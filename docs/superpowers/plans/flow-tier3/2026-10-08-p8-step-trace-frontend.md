# Step Trace Frontend Implementation Plan

> **Execute this plan:** P8. Before starting it, make sure these are merged to main: P7 (must be merged first). After it is merged, the next plan to execute is P9. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the step trace from P7 in the Flow UI: the values that arrived on each wire, how an If or Switch decided, which wire failed, and a duration chip on every non-HTTP node.

**Architecture:** The backend sends a masked, capped `trace` on every step event and summary step. The toolbar copies it into `FlowNodeDetail.trace` next to the existing fields. The Last run tab lists the step's inputs and its route decision. The Wires tab looks up each wire's record in the target node's trace and shows it collapsed. A new `DurationChip` reads `durationMs`, which `toRfNodes` already spreads into node data. All changes are frontend only.

**Tech Stack:** React, TypeScript, Zustand (`pane-store`), shadcn `Badge`, `Button`, `Collapsible`, lucide `Timer`, `ChevronRight`, Vitest and Testing Library.

**Spec:** Roadmap items F-32, F-38 and F-39 (frontend) in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section "P8 Step trace frontend". Backend contract: `docs/superpowers/plans/flow-tier3/2026-10-08-p7-step-trace-backend.md` (Task 1 Interfaces).

## Global Constraints

- Requires P7 merged. The TS types below mirror P7's Rust types exactly (camelCase inside the snake_case event).
- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: narrow selectors only. This plan adds no store action.
- The UI never shows a credential value. A wire record with `credential: true` is shown as "Credential (hidden)" even if a value were present. Every other value is shown exactly as the backend masked it; the UI adds no unmasking and puts values in no `title` or `data-*` attribute.
- Code comments are short full sentences that end with a punctuation mark.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <path>` listed in the task.
- Commits go through the `dev-workflow-skills:1-git-commit` skill with explicit staged paths. Never `git add -A`, `--all` or `.`.
- Only one implementer at a time touches `FlowToolbar.tsx` and `LastRunTab.tsx` (P1, P2, P10 and P15 edit them too). Line numbers are from HEAD b047bbc6; when P1 or P2 merged first, find each edit by the quoted code.
- Not in scope: poll and wait panels and the callback URL (P10), run history (P11), export (P15), the `CopyButton` clipboard switch (P10 or P15).

## Decisions assumed

- No open decision from the index (D1 to D6) affects this plan.
- The Last run tab gets an optional `nodes` prop to label sources. Without it, a source shows its node id.
- An If or Switch step that has no recorded route (a run from before P7) keeps today's "Took: …" line, so old results still read.
- The duration chip sits on its own line under the status caption, not in the header, because `NodeMenuButton` already takes `ml-auto` there.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A credential reaches the DOM: a wire record with `credential: true` and a stray `value` renders the value. Tests pinned in Task 2 (`never shows a value on a credential input`) and Task 3 (`drops a value from a credential wire`, `shows a credential wire as hidden`).
2. An old payload or summary without `trace` crashes a view or loses today's "Took: …" line. Tests pinned in Task 1 (`leaves the trace unset for a step without one`) and Task 2 (`keeps the Took line when no route was recorded`).
3. A very large value (16 KB per wire) is rendered expanded in every Wires row and slows the panel. Test pinned in Task 3 (`keeps the last value collapsed until asked`).
4. The failed-wire highlight lands on the wrong row, or on every row of the step. Test pinned in Task 3 (`highlights only the wire that failed`).
5. An Inputs row for a source node that was deleted after the run crashes or shows nothing. Test pinned in Task 2 (`labels an input from a deleted node by its id`).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/tauri-api.ts` (modify) | `FlowWireValue`, `FlowRouteEval`, `FlowStepTrace`; `trace?` on `FlowStepResult` and `FlowStepCompletedEvent`. |
| `src/types/pane-types.ts` (modify) | `FlowNodeDetail.trace`. |
| `src/components/flow/FlowToolbar.tsx` (modify) | `detailFromEvent` and `detailFromStep` copy `trace`. |
| `src/components/flow/properties/LastRunTab.tsx` (modify) | Inputs list, route line, pass-through and credential notes, value cut note. |
| `src/components/flow/properties/NodePropertiesPanel.tsx` (modify) | Passes `nodes` to `LastRunTab`. |
| `src/components/flow/nodes/DurationChip.tsx` (new) | `formatDuration` and the chip. |
| `src/components/flow/nodes/{Input,Output,Transform,If,Switch,Auth}Node.tsx` (modify) | `durationMs` in node data and the chip. |
| `src/components/flow/properties/wireRows.ts` (modify) | `WireRow.resolved` and `WireRow.failed` from the target node's trace. |
| `src/components/flow/properties/WiresTab.tsx` (modify) | Collapsed last value, hidden credential, wire error, failed-row highlight. |

Existing tests to know: `src/components/flow/__tests__/FlowToolbar.test.tsx` (`renderToolbar`, `started`, `stepHandler`, `resolveRun`), `src/components/flow/properties/__tests__/LastRunTab.test.tsx` (`node()`, `request`, MonacoWrapper mock), `src/components/flow/properties/__tests__/wireRows.test.ts` (`n()`, `edge()`), `src/components/flow/properties/__tests__/WiresTab.test.tsx` (`renderTab`, `nodes`, `edges`), `src/components/flow/nodes/__tests__/IfNode.test.tsx` (`renderIf`), `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx` (`wrap`).

---

### Task 1: Trace types and detail mapping

**Files:**
- Modify: `src/lib/tauri-api.ts` (after `FlowDebugRequest`, which ends at line 2242; `FlowStepResult` :2194-2212; `FlowStepCompletedEvent` :2307-2326)
- Modify: `src/types/pane-types.ts:168-185`
- Modify: `src/components/flow/FlowToolbar.tsx:47-74`
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx` (extend)

**Interfaces:**
- Consumes: P7 JSON `trace: { wires?, route?, failedEdgeId?, valueTruncated? }`.
- Produces: TS `FlowWireValue`, `FlowRouteEval`, `FlowStepTrace` (exported from `@/lib/tauri-api`); `FlowNodeDetail.trace?: FlowStepTrace`. P10 extends `FlowStepTrace` with `poll` and `wait`.

- [ ] **Step 1: Write the failing toolbar tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, add inside `describe('FlowToolbar', ...)`, after the test `'stores the exchange and logs from the step event and the summary'`:

```tsx
  it('stores the trace from the step event and the summary', async () => {
    const trace: tauriApi.FlowStepTrace = {
      wires: [{ edgeId: 'e1', sourceNodeId: 'in', targetField: 'value', value: '••••••' }],
      route: { kind: 'if', value: 'true' },
      failedEdgeId: 'e1',
    };
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'n',
      status: 'success',
      status_code: null,
      duration_ms: 4,
      error: null,
      value: null,
      trace,
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'n',
      'success',
      expect.objectContaining({ trace, durationMs: 4 }),
    );

    resolveRun({
      runId: 'run-123',
      stoppedReason: 'completed',
      steps: [
        {
          nodeId: 'n',
          status: 'success',
          statusCode: null,
          durationMs: 4,
          error: null,
          value: null,
          trace,
        },
      ],
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenLastCalledWith(
        'n',
        'success',
        expect.objectContaining({ trace }),
      ),
    );
  });

  it('leaves the trace unset for a step without one', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'n',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
      value: null,
    });
    const detail = onPatchStatus.mock.calls.at(-1)?.[2];
    expect(detail).toBeDefined();
    expect(detail?.trace).toBeUndefined();
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: `yarn tsc` would fail on the unknown `FlowStepTrace` type and `trace` key; under Vitest the first test FAILS (the detail has no `trace`).

- [ ] **Step 3: Add the types**

In `src/lib/tauri-api.ts`, after the closing `}` of `FlowDebugRequest` (line 2242), insert:

```ts
/** The value one wire delivered to a step. Masked and size-capped by the backend. */
export interface FlowWireValue {
  edgeId: string;
  sourceNodeId: string;
  targetField: string;
  /** Absent for a credential wire and for a wire that failed before it had a value. */
  value?: string;
  /** True when `value` was cut at 16 KB or by the 64 KB per-step budget. */
  truncated?: boolean;
  /** True for an `auth` wire. Its credential is never sent to the UI. */
  credential?: boolean;
  error?: string;
}

/** How an If or Switch node decided. `value` is masked and cut at 1 KB. */
export interface FlowRouteEval {
  kind: 'if' | 'switch';
  /** `true` or `false` for an If, the evaluated value for a Switch. */
  value: string;
  /** The Switch case id that matched. Absent for If and for the default exit. */
  matchedCase?: string;
}

/** What one step saw and decided. Every key is optional. */
export interface FlowStepTrace {
  wires?: FlowWireValue[];
  route?: FlowRouteEval;
  /** The wire whose failure failed the step. */
  failedEdgeId?: string;
  /** True when the step's `value` was cut at 256 KB. */
  valueTruncated?: boolean;
}
```

In `FlowStepResult`, after `attempts?: number;` (line 2211), add:

```ts
  /** Wire values and routing decision of the step, masked and capped. */
  trace?: FlowStepTrace;
```

In `FlowStepCompletedEvent`, after `attempts?: number;` (line 2325), add:

```ts
  /** Wire values and routing decision of the step. Its keys are camelCase. */
  trace?: FlowStepTrace;
```

In `src/types/pane-types.ts`, inside `FlowNodeDetail`, after the `logs` field (line 184), add:

```ts
  /** Wire values and routing decision of the last run, masked by the backend. */
  trace?: import('@/lib/tauri-api').FlowStepTrace;
```

- [ ] **Step 4: Copy the trace in the toolbar**

In `src/components/flow/FlowToolbar.tsx`, in `detailFromEvent`, replace

```tsx
    exchange: event.exchange ?? undefined,
    logs: event.logs?.length ? event.logs : undefined,
  };
}
```

with:

```tsx
    exchange: event.exchange ?? undefined,
    logs: event.logs?.length ? event.logs : undefined,
    trace: event.trace ?? undefined,
  };
}
```

and in `detailFromStep`, replace

```tsx
    exchange: step.exchange ?? undefined,
    logs: step.logs?.length ? step.logs : undefined,
  };
}
```

with:

```tsx
    exchange: step.exchange ?? undefined,
    logs: step.logs?.length ? step.logs : undefined,
    trace: step.trace ?? undefined,
  };
}
```

- [ ] **Step 5: Run the tests**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS, including every existing test (an undefined `trace` key does not affect `toHaveBeenCalledWith` matching).

- [ ] **Step 6: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/tauri-api.ts src/types/pane-types.ts src/components/flow/FlowToolbar.tsx src/components/flow/__tests__/FlowToolbar.test.tsx`
Suggested subject: `feat(flow): keep the step trace on each node's last-run detail`.

---

### Task 2: Last run inputs, route line and duration chips

**Files:**
- Modify: `src/components/flow/properties/LastRunTab.tsx` (imports :1-17, props :19-23, `ValueSection` :240-255, `LastRunTab` :276-327)
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx:236`
- Create: `src/components/flow/nodes/DurationChip.tsx`
- Create: `src/components/flow/nodes/__tests__/DurationChip.test.tsx`
- Modify: `src/components/flow/nodes/InputNode.tsx`, `OutputNode.tsx`, `TransformNode.tsx`, `IfNode.tsx`, `SwitchNode.tsx`, `AuthNode.tsx`
- Test: `src/components/flow/properties/__tests__/LastRunTab.test.tsx` (extend), `src/components/flow/nodes/__tests__/IfNode.test.tsx` (extend), `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowNodeDetail.trace` (Task 1), `fieldLabel` and `exitDisplayLabel` from `./wireRows`.
- Produces: `LastRunTab` prop `nodes?: FlowNode[]`; `DurationChip({ durationMs?: number })`; `formatDuration(ms: number): string`; `durationMs?: number` on the six node data types.

- [ ] **Step 1: Write the failing duration chip tests**

Create `src/components/flow/nodes/__tests__/DurationChip.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DurationChip, formatDuration } from '../DurationChip';

describe('formatDuration', () => {
  it('uses milliseconds under a second and seconds from a second', () => {
    expect(formatDuration(0)).toBe('0ms');
    expect(formatDuration(999)).toBe('999ms');
    expect(formatDuration(1000)).toBe('1s');
    expect(formatDuration(1500)).toBe('1.5s');
  });
});

describe('DurationChip', () => {
  it('renders nothing without a duration', () => {
    const { container } = render(<DurationChip />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows the duration', () => {
    render(<DurationChip durationMs={42} />);
    expect(screen.getByTestId('duration-chip')).toHaveTextContent('42ms');
  });
});
```

In `src/components/flow/nodes/__tests__/IfNode.test.tsx`, add inside `describe('IfNode', ...)`:

```tsx
  it('shows how long the last run took', () => {
    renderIf({ kind, status: 'success', branch: 'true', durationMs: 12 });
    expect(screen.getByTestId('duration-chip')).toHaveTextContent('12ms');
  });

  it('shows no duration before a run', () => {
    renderIf({ kind, status: 'idle' });
    expect(screen.queryByTestId('duration-chip')).not.toBeInTheDocument();
  });
```

In `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`, add inside `describe('InputNode', ...)`:

```tsx
  it('shows how long the last run took', () => {
    wrap(
      <InputNode
        id='i1'
        data={{
          kind: { kind: 'Input', label: 'API Key', value: 'sk-123' },
          status: 'success',
          durationMs: 1500,
        }}
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
    expect(screen.getByTestId('duration-chip')).toHaveTextContent('1.5s');
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/nodes`
Expected: FAIL (cannot resolve `../DurationChip`; no chip in the nodes).

- [ ] **Step 3: Create the chip**

Create `src/components/flow/nodes/DurationChip.tsx`:

```tsx
import { Timer } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { msToSecondsLabel } from '@/lib/flow-repeat';

// Milliseconds under a second, seconds from a second, like the Request card.
export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms}ms` : msToSecondsLabel(ms);
}

// How long a node's last run took. Renders nothing before a run.
export function DurationChip({ durationMs }: { durationMs?: number }) {
  if (durationMs === undefined) return null;
  const text = formatDuration(durationMs);
  return (
    <div className='px-2 pt-1'>
      <Badge
        variant='outline'
        data-testid='duration-chip'
        className='gap-0.5 px-1 py-0 text-[10px] font-normal text-muted-foreground'
      >
        <Timer className='h-2.5 w-2.5' aria-hidden='true' />
        {text}
      </Badge>
    </div>
  );
}
```

- [ ] **Step 4: Add the chip to the six nodes**

In each of `InputNode.tsx`, `OutputNode.tsx`, `TransformNode.tsx`, `IfNode.tsx`, `SwitchNode.tsx` and `AuthNode.tsx` under `src/components/flow/nodes/`:

1. Add the import next to the `NodeStatusCaption` import:

```tsx
import { DurationChip } from './DurationChip';
```

2. In the node's data type (`InputNodeData`, `OutputNodeData`, `TransformNodeData`, `IfNodeData`, `SwitchNodeData`, `AuthNodeData`), add after the `progress` field:

```tsx
  /** How long the last run of this node took. */
  durationMs?: number;
```

3. Directly after the closing `/>` of the `<NodeStatusCaption ... />` element, add:

```tsx
      <DurationChip durationMs={data.durationMs} />
```

`toRfNodes` in `FlowCanvas.tsx` already spreads `nodeDetail[n.id]`, so `durationMs` reaches node data with no canvas change. Request and Wait for callback nodes keep their own timing text and get no chip.

- [ ] **Step 5: Run the node tests**

Run: `yarn test src/components/flow/nodes`
Expected: PASS.

- [ ] **Step 6: Write the failing Last run tests**

In `src/components/flow/properties/__tests__/LastRunTab.test.tsx`, change the type import on line 4 to:

```tsx
import type {
  FlowDebugRequest,
  FlowDebugResponse,
  FlowNode,
  FlowNodeKind,
  FlowStepTrace,
} from '@/lib/tauri-api';
```

and append:

```tsx
describe('LastRunTab trace', () => {
  const login = node({
    kind: 'Request',
    label: 'Login',
    source: { type: 'Saved', requestPath: 'auth/login.yml' },
  });
  const sourceNode: FlowNode = {
    id: 'src',
    kind: { kind: 'Input', label: 'API Key', value: '{{apiKey}}' },
    position: { x: 0, y: 0 },
  };

  it('lists the inputs the step received, as the backend masked them', () => {
    const trace: FlowStepTrace = {
      wires: [
        { edgeId: 'e1', sourceNodeId: 'src', targetField: 'headers[X-Key].value', value: '••••••' },
        { edgeId: 'e2', sourceNodeId: 'src', targetField: 'body', value: 'x'.repeat(10), truncated: true },
      ],
    };
    render(<LastRunTab node={login} nodes={[sourceNode]} status='success' detail={{ trace }} />);
    const rows = screen.getAllByTestId('last-run-input');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('X-Key');
    expect(rows[0]).toHaveTextContent('← API Key');
    expect(rows[0]).toHaveTextContent('••••••');
    expect(rows[1]).toHaveTextContent('Cut at 16 KB.');
  });

  it('never shows a value on a credential input', () => {
    const trace: FlowStepTrace = {
      wires: [
        {
          edgeId: 'ea',
          sourceNodeId: 'src',
          targetField: 'auth',
          credential: true,
          value: 'leaked-token-123456',
        },
      ],
    };
    render(<LastRunTab node={login} status='success' detail={{ trace }} />);
    expect(screen.getByTestId('last-run-input')).toHaveTextContent('Credential (hidden)');
    expect(document.body).not.toHaveTextContent('leaked-token-123456');
  });

  it('marks the input that failed and shows its error', () => {
    const trace: FlowStepTrace = {
      wires: [
        { edgeId: 'e1', sourceNodeId: 'src', targetField: 'url', value: 'ok' },
        { edgeId: 'e2', sourceNodeId: 'src', targetField: 'body', error: 'ReferenceError: x' },
      ],
      failedEdgeId: 'e2',
    };
    render(<LastRunTab node={login} status='failed' detail={{ error: 'wire failed', trace }} />);
    const rows = screen.getAllByTestId('last-run-input');
    expect(rows[0]).not.toHaveAttribute('data-failed');
    expect(rows[1]).toHaveAttribute('data-failed', 'true');
    expect(screen.getByTestId('last-run-input-error')).toHaveTextContent('ReferenceError: x');
  });

  it('labels an input from a deleted node by its id', () => {
    const trace: FlowStepTrace = {
      wires: [{ edgeId: 'e1', sourceNodeId: 'gone', targetField: 'url', value: 'v' }],
    };
    render(<LastRunTab node={login} nodes={[]} status='success' detail={{ trace }} />);
    expect(screen.getByTestId('last-run-input')).toHaveTextContent('← gone');
  });

  it('shows the If condition result', () => {
    const ifNode = node({ kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
    render(
      <LastRunTab
        node={ifNode}
        status='success'
        detail={{ branch: 'true', trace: { route: { kind: 'if', value: 'true' } } }}
      />,
    );
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Condition → true');
    expect(screen.getByText('Passes its input through.')).toBeInTheDocument();
  });

  it('shows the Switch value and the case it matched', () => {
    const sw = node({
      kind: 'Switch',
      label: 'Type',
      value: 'response.body.type',
      cases: [{ id: 'c1', label: 'Admin', matches: 'admin' }],
    });
    render(
      <LastRunTab
        node={sw}
        status='success'
        detail={{
          branch: 'case:c1',
          trace: { route: { kind: 'switch', value: 'admin', matchedCase: 'c1' } },
        }}
      />,
    );
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Value admin → case Admin');
  });

  it('shows a Switch that fell to the default exit', () => {
    const sw = node({ kind: 'Switch', label: 'Type', value: 'x', cases: [] });
    render(
      <LastRunTab
        node={sw}
        status='success'
        detail={{ branch: 'default', trace: { route: { kind: 'switch', value: 'pro' } } }}
      />,
    );
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Value pro → default');
  });

  it('keeps the Took line when no route was recorded', () => {
    const ifNode = node({ kind: 'If', label: 'Ok?', condition: 'true' });
    render(<LastRunTab node={ifNode} status='success' detail={{ branch: 'false' }} />);
    expect(screen.getByTestId('last-run-branch')).toHaveTextContent('Took: false');
  });

  it('says an Auth credential is hidden', () => {
    const auth = node({
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: true,
    });
    render(<LastRunTab node={auth} status='success' detail={{ durationMs: 1 }} />);
    expect(screen.getByText('The credential is hidden.')).toBeInTheDocument();
  });

  it('notes a value that was cut', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(
      <LastRunTab
        node={out}
        status='success'
        detail={{ value: 'abc', trace: { valueTruncated: true } }}
      />,
    );
    expect(screen.getByText('Cut at 256 KB.')).toBeInTheDocument();
  });

  it('shows the duration of a non-HTTP node', () => {
    const out = node({ kind: 'Output', label: 'Token' });
    render(<LastRunTab node={out} status='success' detail={{ value: 'abc', durationMs: 3 }} />);
    expect(screen.getByTestId('last-run-status')).toHaveTextContent('3ms');
  });
});
```

- [ ] **Step 7: Run them to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/LastRunTab.test.tsx`
Expected: the new trace tests FAIL (no inputs list, no route line, no notes). "shows the duration of a non-HTTP node" may already pass (`timingParts` prints `Nms`).

- [ ] **Step 8: Implement the Last run additions**

In `src/components/flow/properties/LastRunTab.tsx`:

1. Change the type import (lines 9-15) to:

```tsx
import type {
  FlowDebugHeader,
  FlowDebugRequest,
  FlowLogEntry,
  FlowNode,
  FlowNodeStatus,
  FlowRouteEval,
  FlowWireValue,
} from '@/lib/tauri-api';
```

add `import { cn } from '@/lib/utils';` right after that `@/lib/tauri-api` type import (Biome keeps imports sorted by path; `yarn lint` fixes the order if needed), and change line 17 to:

```tsx
import { exitDisplayLabel, fieldLabel } from './wireRows';
```

2. Change `LastRunTabProps` (lines 19-23) to:

```tsx
interface LastRunTabProps {
  node: FlowNode;
  status: FlowNodeStatus;
  detail?: FlowNodeDetail;
  /** The flow's nodes, to name the source of each input. */
  nodes?: FlowNode[];
}
```

3. Replace `ValueSection` (lines 240-255) with:

```tsx
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
```

4. Change the `LastRunTab` signature (line 276) to:

```tsx
export function LastRunTab({ node, status, detail, nodes }: LastRunTabProps) {
```

5. Replace the block from `{status === 'skipped' && ...}` (line 306) to the end of the Value block (line 323):

```tsx
      {status === 'skipped' && <p className='text-muted-foreground'>{skipText(detail)}</p>}
      {(node.kind.kind === 'Request' || node.kind.kind === 'WaitForCallback') &&
        detail?.exchange && (
          <ExchangeSections
            exchange={detail.exchange}
            shownError={status === 'failed' ? detail.error : undefined}
            received={node.kind.kind === 'WaitForCallback'}
          />
        )}
      {(node.kind.kind === 'If' || node.kind.kind === 'Switch') && detail?.branch && (
        <p data-testid='last-run-branch'>
          Took: <span className='font-mono'>{exitDisplayLabel(node, detail.branch)}</span>
        </p>
      )}
      {(node.kind.kind === 'Output' ||
        node.kind.kind === 'Input' ||
        node.kind.kind === 'Transform') &&
        detail?.value !== undefined && <ValueSection value={detail.value} />}
```

with:

```tsx
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
```

6. In `src/components/flow/properties/NodePropertiesPanel.tsx`, change line 236 to:

```tsx
            <LastRunTab node={node} status={status} detail={detail} nodes={nodes} />
```

- [ ] **Step 9: Run the tests**

Run: `yarn test src/components/flow/properties src/components/flow/nodes`
Expected: PASS, including the existing "Took: true", "Took: Admin" and "(deleted case)" tests (they have no route).

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/properties/LastRunTab.tsx src/components/flow/properties/NodePropertiesPanel.tsx src/components/flow/properties/__tests__/LastRunTab.test.tsx src/components/flow/nodes/DurationChip.tsx src/components/flow/nodes/__tests__/DurationChip.test.tsx src/components/flow/nodes/InputNode.tsx src/components/flow/nodes/OutputNode.tsx src/components/flow/nodes/TransformNode.tsx src/components/flow/nodes/IfNode.tsx src/components/flow/nodes/SwitchNode.tsx src/components/flow/nodes/AuthNode.tsx src/components/flow/nodes/__tests__/IfNode.test.tsx src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`
Suggested subject: `feat(flow): show step inputs, route decisions and durations`.

---

### Task 3: Resolved wire values in the Wires tab

**Files:**
- Modify: `src/components/flow/properties/wireRows.ts` (imports :1-10, `WireRow` :12-21, `row()` :90-115)
- Modify: `src/components/flow/properties/WiresTab.tsx` (imports :2-8, `Row` :44-100)
- Test: `src/components/flow/properties/__tests__/wireRows.test.ts` (extend), `src/components/flow/properties/__tests__/WiresTab.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowNodeDetail.trace` (Task 1).
- Produces: `WireResolved { value?: string; truncated: boolean; credential: boolean; error?: string }`; `WireRow.resolved?: WireResolved`; `WireRow.failed: boolean`. The record is read from the target node's detail: `nodeDetail[edge.targetNodeId].trace.wires`, matched by `edgeId`.

- [ ] **Step 1: Write the failing row tests**

In `src/components/flow/properties/__tests__/wireRows.test.ts`, append:

```ts
describe('last run values', () => {
  const into = edge({
    id: 'e1',
    sourceNodeId: 'login',
    targetNodeId: 'users',
    targetField: 'headers[Authorization].value',
    expression: 'response.body.token',
  });

  it('reads the value from the target node trace for incoming and outgoing rows', () => {
    const detail = {
      users: { trace: { wires: [{ edgeId: 'e1', sourceNodeId: 'login', targetField: 'x', value: '••••••' }] } },
    };
    const [incoming] = incomingRows(users, [login, users], [into], {}, detail);
    expect(incoming.resolved).toEqual({ value: '••••••', truncated: false, credential: false });
    const [group] = outgoingGroups(login, [login, users], [into], {}, detail);
    expect(group.rows[0].resolved?.value).toBe('••••••');
  });

  it('has no value before a run', () => {
    const [row] = incomingRows(users, [login, users], [into]);
    expect(row.resolved).toBeUndefined();
    expect(row.failed).toBe(false);
  });

  it('drops a value from a credential wire', () => {
    const detail = {
      users: {
        trace: {
          wires: [
            { edgeId: 'e1', sourceNodeId: 'login', targetField: 'auth', credential: true, value: 'tok-123456' },
          ],
        },
      },
    };
    const [row] = incomingRows(users, [login, users], [into], {}, detail);
    expect(row.resolved).toEqual({ value: undefined, truncated: false, credential: true });
  });

  it('marks the wire that failed', () => {
    const second = edge({ id: 'e2', sourceNodeId: 'login', targetNodeId: 'users', targetField: 'url' });
    const detail = {
      users: {
        trace: {
          wires: [{ edgeId: 'e2', sourceNodeId: 'login', targetField: 'url', error: 'boom' }],
          failedEdgeId: 'e2',
        },
      },
    };
    const rows = incomingRows(users, [login, users], [into, second], {}, detail);
    expect(rows.map((r) => r.failed)).toEqual([false, true]);
    expect(rows[1].resolved?.error).toBe('boom');
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/wireRows.test.ts`
Expected: FAIL (`resolved` and `failed` are missing).

- [ ] **Step 3: Read the record in `row()`**

In `src/components/flow/properties/wireRows.ts`, add after `WireRow`'s `notTaken: boolean;` line:

```ts
  /** What the last run delivered on this wire, when the target node recorded it. */
  resolved?: WireResolved;
  /** True when this wire failed its target node in the last run. */
  failed: boolean;
```

and above `export interface WireRow`:

```ts
/** What the last run delivered on a wire, as the backend masked it. */
export interface WireResolved {
  value?: string;
  truncated: boolean;
  credential: boolean;
  error?: string;
}
```

In `row()`, after the line `const exitLabel = handle === null ? null : exitDisplayLabel(source, handle);`, add:

```ts
  // The target node's step recorded what arrived on this wire.
  const targetTrace = nodeDetail?.[edge.targetNodeId]?.trace;
  const recorded = targetTrace?.wires?.find((w) => w.edgeId === edge.id);
```

and in the returned object, after `notTaken: isNotTaken(edge, source, nodeStatus, nodeDetail),`, add:

```ts
    resolved: recorded && {
      // A credential wire never shows a value, even if one arrived.
      value: recorded.credential ? undefined : recorded.value,
      truncated: recorded.truncated ?? false,
      credential: recorded.credential ?? false,
      error: recorded.error,
    },
    failed: targetTrace?.failedEdgeId === edge.id,
```

- [ ] **Step 4: Run the row tests**

Run: `yarn test src/components/flow/properties/__tests__/wireRows.test.ts`
Expected: PASS, including the existing tests (they use `expect.objectContaining` for rows).

- [ ] **Step 5: Write the failing tab tests**

In `src/components/flow/properties/__tests__/WiresTab.test.tsx`, append inside `describe('WiresTab', ...)`:

```tsx
  it('keeps the last value collapsed until asked', async () => {
    renderTab(out, {
      nodeStatus: { out: 'success' },
      nodeDetail: {
        out: {
          trace: {
            wires: [
              { edgeId: 'e3', sourceNodeId: 'check', targetField: 'value', value: 'big value', truncated: true },
            ],
          },
        },
      },
    });
    expect(screen.queryByTestId('wire-value')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Last value/ }));
    expect(screen.getByTestId('wire-value')).toHaveTextContent('big value');
    expect(screen.getByText('Cut at 16 KB.')).toBeInTheDocument();
  });

  it('shows a credential wire as hidden', () => {
    const auth = n('signin', {
      kind: 'Auth',
      label: 'Sign in',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: false,
    });
    render(
      <WiresTab
        node={users}
        nodes={[auth, users]}
        edges={[
          { id: 'ea', sourceNodeId: 'signin', targetNodeId: 'users', targetField: 'auth', expression: '' },
        ]}
        nodeDetail={{
          users: {
            trace: {
              wires: [
                {
                  edgeId: 'ea',
                  sourceNodeId: 'signin',
                  targetField: 'auth',
                  credential: true,
                  value: 'leaked-token-123456',
                },
              ],
            },
          },
        }}
        onEditWire={vi.fn()}
        onSelectNode={vi.fn()}
      />,
    );
    expect(screen.getByTestId('wire-credential')).toHaveTextContent('Credential (hidden)');
    expect(screen.queryByRole('button', { name: /Last value/ })).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent('leaked-token-123456');
  });

  it('highlights only the wire that failed', () => {
    renderTab(check, {
      nodeStatus: { check: 'failed' },
      nodeDetail: {
        check: {
          trace: {
            wires: [{ edgeId: 'e1', sourceNodeId: 'login', targetField: 'input', error: 'boom' }],
            failedEdgeId: 'e1',
          },
        },
      },
    });
    const incoming = within(screen.getByTestId('wires-incoming')).getByTestId('wire-row');
    expect(incoming).toHaveAttribute('data-failed', 'true');
    expect(within(incoming).getByTestId('wire-error')).toHaveTextContent('boom');
    for (const row of within(screen.getByTestId('wires-outgoing')).getAllByTestId('wire-row')) {
      expect(row).not.toHaveAttribute('data-failed');
    }
  });
```

- [ ] **Step 6: Run them to verify they fail**

Run: `yarn test src/components/flow/properties/__tests__/WiresTab.test.tsx`
Expected: the three new tests FAIL.

- [ ] **Step 7: Render the resolved value**

In `src/components/flow/properties/WiresTab.tsx`:

1. Change the imports (lines 2-8) to:

```tsx
import { ArrowLeft, ArrowRight, ChevronRight, Pencil } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import type { FlowNodeDetail } from '@/types/pane-types';
import { usePanelRefocus } from './panelFocus';
import { incomingRows, outgoingGroups, type WireResolved, type WireRow } from './wireRows';
```

2. Before `function Row(`, add:

```tsx
// The value the last run put on a wire. Collapsed, because one can be 16 KB.
function ResolvedValue({ resolved }: { resolved: WireResolved }) {
  const [open, setOpen] = useState(false);
  if (resolved.credential) {
    return (
      <p data-testid='wire-credential' className='italic text-muted-foreground'>
        Credential (hidden)
      </p>
    );
  }
  if (resolved.error) {
    return (
      <p
        data-testid='wire-error'
        className='select-text whitespace-pre-wrap break-words text-red-600'
      >
        {resolved.error}
      </p>
    );
  }
  if (resolved.value === undefined) return null;
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <Button type='button' variant='ghost' size='sm' className='h-5 px-1 text-[11px]'>
          <ChevronRight
            className={open ? 'h-3 w-3 rotate-90 transition-transform' : 'h-3 w-3 transition-transform'}
            aria-hidden='true'
          />
          Last value
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <pre
          data-testid='wire-value'
          className='mt-1 max-h-40 select-text overflow-auto whitespace-pre-wrap rounded-md border p-1.5 font-mono text-[11px] [overflow-wrap:anywhere]'
        >
          {resolved.value === '' ? <span className='italic'>(empty)</span> : resolved.value}
        </pre>
        {resolved.truncated && <p className='text-muted-foreground'>Cut at 16 KB.</p>}
      </CollapsibleContent>
    </Collapsible>
  );
}
```

3. In `Row`, replace the opening element

```tsx
    <div
      data-testid='wire-row'
      className={cn(
        'flex items-start gap-1.5 rounded-md border px-2 py-1.5',
        row.notTaken && 'opacity-50',
      )}
    >
```

with:

```tsx
    <div
      data-testid='wire-row'
      data-failed={row.failed ? 'true' : undefined}
      className={cn(
        'flex items-start gap-1.5 rounded-md border px-2 py-1.5',
        row.notTaken && 'opacity-50',
        row.failed && 'border-red-500/60 bg-red-500/5',
      )}
    >
```

and replace

```tsx
        <p className='truncate font-mono text-[11px] text-muted-foreground'>
          {row.preview ?? '(no script)'}
        </p>
      </div>
```

with:

```tsx
        <p className='truncate font-mono text-[11px] text-muted-foreground'>
          {row.preview ?? '(no script)'}
        </p>
        {row.resolved && <ResolvedValue resolved={row.resolved} />}
      </div>
```

- [ ] **Step 8: Run the tests**

Run: `yarn test src/components/flow/properties`
Expected: PASS.

- [ ] **Step 9: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/properties/wireRows.ts src/components/flow/properties/WiresTab.tsx src/components/flow/properties/__tests__/wireRows.test.ts src/components/flow/properties/__tests__/WiresTab.test.tsx`
Suggested subject: `feat(flow): show each wire's last value in the Wires tab`.

---

## Self-Review

- **Spec coverage:** TS types and `trace` on the summary step and the event, `FlowNodeDetail.trace`, both detail mappers (Task 1). Read-only Inputs list, one-line notes for If, Switch and Auth, "Condition → true" and "Value `x` → case Y or default", value cut note, `DurationChip` on Input, Output, Transform, If, Switch and Auth with the Last run line already printing `Nms` (Task 2). `WireRow.resolved` from the target node's trace, collapsed value, "Credential (hidden)", red error, failed-row highlight (Task 3).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowWireValue`, `FlowRouteEval`, `FlowStepTrace` match P7's Rust fields in camelCase. `WireResolved` is used the same way in `wireRows.ts`, `WiresTab.tsx` and both test files. `LastRunTab`'s new `nodes` prop is optional, so P10 and existing tests compile unchanged.
- **Review Focus coverage:** item 1 in Task 2 and Task 3, item 2 in Task 1 and Task 2, item 3 in Task 3, item 4 in Task 3, item 5 in Task 2.

Known follow-ups outside this plan: the design notes suggested a `FlowCanvas.routing.test.tsx` duration test; the chip is covered at node level instead, because `toRfNodes` already spreads `durationMs` and has its own tests.
