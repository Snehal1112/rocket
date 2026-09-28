# Flow Phase 2 — Plan 04: Frontend Types, Store and Wiring — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the frontend the data plumbing for If/Switch routing: new types, handle constants, a complete per-node run-detail type, exit-aware edges, client-side connection rules, "Run when" trigger inputs, generic validation-error highlighting, and skip-reason/branch forwarding from runs.

**Architecture:** Pure helpers live in `src/lib/flow-handles.ts` (handle vocabulary, mirroring `rocket_flow::handle`) and `src/lib/flow-wiring.ts` (edge building, connection rules, error parsing). Components only call these helpers. No new node components and no status styling here. Plan 05 owns `IfNode`/`SwitchNode`, the palette and run visualization.

**Tech Stack:** React 18 + TypeScript, `@xyflow/react` 12, Zustand (`pane-store`), Vitest + Testing Library, Biome.

**Spec:** `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` (§8.4, §9.2, and the data parts of §9.3). Cross-plan names: `docs/superpowers/plans/flow-phase2-branching/00-plan-index.md`.

**Prerequisites:** Plans 01–03 are merged. The backend accepts `sourceHandle`, If/Switch kinds and `trigger`/`input` target fields, and emits `skip_reason`/`branch`/`not_taken_count`.

## Global Constraints

- UI uses shadcn/ui primitives only (no raw `<button>`, `<input>`, `<select>`, `<form>`, `<dialog>`). Icons come from `lucide-react` only.
- Zustand: use narrow selectors (`usePaneStore((s) => s.x)`). Never destructure the whole store at the top of a component.
- Handle strings must match the backend exactly: `result`, `true`, `false`, `default`, `input`, `trigger`, `case:<id>`.
- `sourceHandle` is **omitted** when it is `result` (the backend omits it too), so Phase 1 flows re-save unchanged.
- Event payloads (`flow-step-completed` etc.) are snake_case (`skip_reason`, `branch`, `not_taken_count`). The `run_flow` summary is camelCase (`skipReason`, `branch`).
- Package manager is `yarn`. Biome style: 2 spaces, single quotes (including JSX), trailing commas, 100-column lines.
- Comments are short full sentences ending with a period.

## Review Focus

1. **A Phase 1 flow edited and re-saved.** A new wire from a plain node must not gain `sourceHandle: 'result'`. Task 2 has a test asserting the key is absent.
2. **React Flow reports `sourceHandle: null`** (this happens for nodes with one unnamed source handle). It must be treated as `result`: valid from a Request node, invalid from an If node. Task 2 has a test.
3. **A second wire into an If/Switch `input`, a wire into an Input node, or `input` dropped on a Request node.** Each must be refused while dragging. Task 2 has a test.
4. **A wire from a Switch case that was deleted, or a malformed `case:` handle with an empty id.** Refused. Covered by tests in Task 1 (`caseIdFromHandle`) and Task 2 (`isValidFlowConnection`).
5. **A non-cycle validation error, where either id list is empty** (`… node(s): b; edge(s): `). The parser must return `[]`, not `['']`, and the canvas must still flag node `b`. Task 2 has tests in both the parser and `FlowPane`.

---

### Task 1: Types, handle vocabulary and `FlowNodeDetail`

**Files:**
- Modify: `src/lib/tauri-api.ts` (Flow section, currently lines ~1728–1886)
- Create: `src/lib/flow-handles.ts`
- Create: `src/lib/__tests__/flow-handles.test.ts`
- Modify: `src/types/pane-types.ts:155-166` (`FlowTab`)
- Modify: `src/stores/pane-store.ts:231-236` (`patchFlowNodeStatus` signature)
- Modify: `src/components/flow/FlowCanvas.tsx:43-47, 69-72` (use `FlowNodeDetail`)
- Modify: `src/components/flow/FlowToolbar.tsx:14` (use `FlowNodeDetail`)
- Test: `src/stores/__tests__/pane-store.test.ts` ("Flow tab actions" suite, ~line 833)

**Interfaces:**
- Consumes: the backend wire shapes from plans 01–02 (spec §8).
- Produces:
  - `tauri-api.ts`:
    - `SwitchCase`, the `If`/`Switch` members of `FlowNodeKind`, `FlowEdge.sourceHandle?: string`, `FlowSkipReason`.
    - Summary fields `FlowStepResult.skipReason?`/`branch?`.
    - Event fields `FlowStepCompletedEvent.skip_reason?`/`branch?` and `FlowRunFinishedEvent.not_taken_count?`.
  - `flow-handles.ts`: `RESULT_HANDLE`, `TRUE_HANDLE`, `FALSE_HANDLE`, `DEFAULT_HANDLE`, `INPUT_HANDLE`, `TRIGGER_HANDLE`, `caseHandle(caseId: string): string`, `caseIdFromHandle(handle: string): string | null`, `type RoutingKind`, `isRoutingKind(kind: FlowNodeKind): kind is RoutingKind`.
  - `pane-types.ts`: `export interface FlowNodeDetail { statusCode?; durationMs?; error?; value?; skipReason?; branch? }`, plus `FlowTab.nodeDetail?: Record<string, FlowNodeDetail>`.
  - `pane-store.ts`: `patchFlowNodeStatus(tabId, nodeId, status, detail?: FlowNodeDetail)`.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-handles.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowNodeKind } from '@/lib/tauri-api';
import {
  caseHandle,
  caseIdFromHandle,
  DEFAULT_HANDLE,
  FALSE_HANDLE,
  INPUT_HANDLE,
  isRoutingKind,
  RESULT_HANDLE,
  TRIGGER_HANDLE,
  TRUE_HANDLE,
} from '../flow-handles';

describe('flow handle vocabulary', () => {
  it('uses the exact strings the backend expects', () => {
    expect([
      RESULT_HANDLE,
      TRUE_HANDLE,
      FALSE_HANDLE,
      DEFAULT_HANDLE,
      INPUT_HANDLE,
      TRIGGER_HANDLE,
    ]).toEqual(['result', 'true', 'false', 'default', 'input', 'trigger']);
  });

  it('builds and reads a case handle', () => {
    expect(caseHandle('01J9CASE')).toBe('case:01J9CASE');
    expect(caseIdFromHandle('case:01J9CASE')).toBe('01J9CASE');
  });

  it('returns null for a non-case handle or a case handle with no id', () => {
    expect(caseIdFromHandle('default')).toBeNull();
    expect(caseIdFromHandle('result')).toBeNull();
    expect(caseIdFromHandle('case:')).toBeNull();
  });

  it('recognises only If and Switch as routing kinds', () => {
    const kinds: FlowNodeKind[] = [
      { kind: 'If', label: 'i', condition: 'true' },
      { kind: 'Switch', label: 's', value: 'x', cases: [] },
      { kind: 'Output', label: 'o' },
      { kind: 'Input', label: 'in', value: 'v' },
      { kind: 'Request', label: 'r', source: { type: 'Saved', requestPath: 'a.yml' } },
    ];
    expect(kinds.map(isRoutingKind)).toEqual([true, true, false, false, false]);
  });
});
```

Add this test to the `describe('Flow tab actions', …)` suite in `src/stores/__tests__/pane-store.test.ts`, after `'patchFlowNodeStatus records optional detail alongside the status'`:

```ts
  it('patchFlowNodeStatus keeps skip reason, branch and value in the detail', async () => {
    await usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()?.id;
    if (!tabId) throw new Error('Expected a flow tab');
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    usePaneStore
      .getState()
      .patchFlowNodeStatus(tabId, 'n1', 'skipped', { skipReason: 'branch_not_taken' });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ skipReason: 'branch_not_taken' });

    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'success', { branch: 'true', value: '42' });
    expect(findFirstFlowTab()?.nodeDetail?.n1).toEqual({ branch: 'true', value: '42' });
  });
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `yarn test --run flow-handles`
Expected: FAIL, `Failed to resolve import "../flow-handles"`.

Run: `yarn tsc --noEmit`
Expected: FAIL. The new store test reports `Object literal may only specify known properties, and 'skipReason' does not exist in type '{ statusCode?: number; durationMs?: number; error?: string; }'`. The flow-handles test also reports `Type '"If"' is not assignable to type …`.

- [ ] **Step 3: Extend the IPC types in `src/lib/tauri-api.ts`**

Replace the `FlowNodeKind` union and the `FlowEdge` interface:

```ts
/** One Switch case. Edges leave a case through the handle `case:<id>`. */
export interface SwitchCase {
  id: string;
  label: string;
  matches: string;
}

export type FlowNodeKind =
  | { kind: 'Request'; label: string; source: RequestSource }
  | { kind: 'Input'; label: string; value: unknown }
  | { kind: 'Output'; label: string }
  | { kind: 'If'; label: string; condition: string }
  | { kind: 'Switch'; label: string; value: string; cases: SwitchCase[] };
```

```ts
export interface FlowEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  targetField: string;
  expression: string;
  /** Exit of the source node the edge leaves from. Absent means `result`. */
  sourceHandle?: string;
}
```

Directly under `export type FlowRunNodeStatus = …`, add:

```ts
/** Why a skipped node did not run. Only set on skipped steps. */
export type FlowSkipReason = 'upstream_failed' | 'branch_not_taken';
```

Add two optional fields to `FlowStepResult`, after `value: string | null;`:

```ts
  skipReason?: FlowSkipReason;
  /** Exit a completed If/Switch node took: `true`, `false`, `case:<id>` or `default`. */
  branch?: string;
```

Add two optional fields to `FlowStepCompletedEvent`, after `value: string | null;`:

```ts
  skip_reason?: FlowSkipReason;
  branch?: string;
```

Add one optional field to `FlowRunFinishedEvent`, after `skipped_count: number;`:

```ts
  /** How many of `skipped_count` were skipped because their branch was not taken. */
  not_taken_count?: number;
```

- [ ] **Step 4: Create `src/lib/flow-handles.ts`**

```ts
import type { FlowNodeKind } from '@/lib/tauri-api';

// These strings must match rocket_flow::handle in the backend.
export const RESULT_HANDLE = 'result';
export const TRUE_HANDLE = 'true';
export const FALSE_HANDLE = 'false';
export const DEFAULT_HANDLE = 'default';
export const INPUT_HANDLE = 'input';
export const TRIGGER_HANDLE = 'trigger';
const CASE_PREFIX = 'case:';

export function caseHandle(caseId: string): string {
  return `${CASE_PREFIX}${caseId}`;
}

/** Returns the case id of a `case:<id>` handle, or null for any other handle. */
export function caseIdFromHandle(handle: string): string | null {
  if (!handle.startsWith(CASE_PREFIX)) return null;
  const id = handle.slice(CASE_PREFIX.length);
  return id ? id : null;
}

export type RoutingKind = Extract<FlowNodeKind, { kind: 'If' | 'Switch' }>;

export function isRoutingKind(kind: FlowNodeKind): kind is RoutingKind {
  return kind.kind === 'If' || kind.kind === 'Switch';
}
```

- [ ] **Step 5: Add `FlowNodeDetail` and use it everywhere run detail flows**

In `src/types/pane-types.ts`, directly above `export interface FlowTab`, add:

```ts
/** Per-node result of the last run. Every field is optional. */
export interface FlowNodeDetail {
  statusCode?: number;
  durationMs?: number;
  error?: string;
  /** Captured value shown by an Output node. */
  value?: string;
  skipReason?: import('@/lib/tauri-api').FlowSkipReason;
  /** Exit a routing node took. */
  branch?: string;
}
```

In `FlowTab`, replace the `nodeDetail` line:

```ts
  nodeDetail?: Record<string, FlowNodeDetail>;
```

In `src/stores/pane-store.ts`, add `FlowNodeDetail` to the existing `import type { … } from '@/types/pane-types'` list, then change the `patchFlowNodeStatus` signature in the store interface:

```ts
  patchFlowNodeStatus: (
    tabId: string,
    nodeId: string,
    status: FlowNodeStatus,
    detail?: FlowNodeDetail,
  ) => void;
```

The implementation (~line 781) stays unchanged. It already stores whatever `detail` it gets.

In `src/components/flow/FlowCanvas.tsx`, add `import type { FlowNodeDetail } from '@/types/pane-types';`. Replace both inline detail types: the `nodeDetail?:` prop in `FlowCanvasProps`, and the `nodeDetail?:` parameter of `toRfNodes`. Both become:

```ts
  nodeDetail?: Record<string, FlowNodeDetail>;
```

In `src/components/flow/FlowToolbar.tsx`, delete the line `type NodeDetail = { … };`. Add `import type { FlowNodeDetail } from '@/types/pane-types';` and change the prop:

```ts
  onPatchStatus: (nodeId: string, status: string, detail?: FlowNodeDetail) => void;
```

- [ ] **Step 6: Run the checks and verify they pass**

Run: `yarn tsc --noEmit`
Expected: no output, exit code 0.

Run: `yarn test --run flow-handles pane-store`
Expected: PASS, including `patchFlowNodeStatus keeps skip reason, branch and value in the detail`.

- [ ] **Step 7: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/flow-handles.ts src/lib/__tests__/flow-handles.test.ts \
  src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts \
  src/components/flow/FlowCanvas.tsx src/components/flow/FlowToolbar.tsx
```

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add routing node types and handle vocabulary`.

---

### Task 2: Wiring helpers: exits, connection rules, generic error parsing

**Files:**
- Modify: `src/lib/flow-wiring.ts` (whole file)
- Modify: `src/lib/__tests__/flow-wiring.test.ts`
- Modify: `src/components/flow/FlowPane.tsx:14, 191-200` (rename the parser call)
- Test: `src/components/flow/__tests__/FlowPane.test.tsx` ("FlowPane save" suite)

**Interfaces:**
- Consumes: from Task 1, the `flow-handles.ts` constants, `caseIdFromHandle`, `isRoutingKind`, and `FlowEdge.sourceHandle`.
- Produces:
  - `buildEdgeFromConnection(connection, sourceNode): FlowEdge | null`. It keeps a non-`result` `sourceHandle`, and uses expression `''` for data-less targets.
  - `isDataLessTarget(targetHandle: string): boolean`.
  - `shouldPromptForExpression(edge: FlowEdge): boolean`.
  - `type ConnectionLike = { source: string | null; target: string | null; sourceHandle?: string | null; targetHandle?: string | null }`.
  - `isValidFlowConnection(connection: ConnectionLike, nodes: FlowNode[], edges: FlowEdge[]): boolean`.
  - `interface GraphErrorIds { nodeIds: string[]; edgeIds: string[] }`.
  - `parseGraphErrorMessage(message: string): GraphErrorIds | null`. It replaces `parseCycleErrorMessage` and `CycleError`, which are deleted.

- [ ] **Step 1: Write the failing tests**

Replace the import block and the `describe('parseCycleErrorMessage', …)` block in `src/lib/__tests__/flow-wiring.test.ts`. Keep the existing `defaultExpressionFor` and `buildEdgeFromConnection` tests. New import block:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import {
  buildEdgeFromConnection,
  defaultExpressionFor,
  isDataLessTarget,
  isValidFlowConnection,
  parseGraphErrorMessage,
  shouldPromptForExpression,
} from '../flow-wiring';
```

Append these blocks:

```ts
const node = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const req = node('req', {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});
const inp = node('inp', { kind: 'Input', label: 'User', value: 'alice' });
const out = node('out', { kind: 'Output', label: 'Out' });
const iff = node('iff', { kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
const sw = node('sw', {
  kind: 'Switch',
  label: 'Plan',
  value: 'response.body.plan',
  cases: [{ id: 'c1', label: 'Pro', matches: 'pro' }],
});
const nodes = [req, inp, out, iff, sw];

describe('buildEdgeFromConnection with exits', () => {
  it('omits sourceHandle for the default result exit', () => {
    const edge = buildEdgeFromConnection(
      { source: 'req', sourceHandle: 'result', target: 'out', targetHandle: 'value' },
      req,
    );
    expect(edge).not.toBeNull();
    expect(edge && 'sourceHandle' in edge).toBe(false);
  });

  it('omits sourceHandle when React Flow reports a null source handle', () => {
    const edge = buildEdgeFromConnection(
      { source: 'req', sourceHandle: null, target: 'out', targetHandle: 'value' },
      req,
    );
    expect(edge && 'sourceHandle' in edge).toBe(false);
  });

  it('keeps a routing exit as sourceHandle', () => {
    const edge = buildEdgeFromConnection(
      { source: 'iff', sourceHandle: 'true', target: 'req', targetHandle: 'url' },
      iff,
    );
    expect(edge).toMatchObject({ sourceHandle: 'true', targetField: 'url' });
    expect(edge?.expression).toBe('response.body');
  });

  it('gives input and trigger wires an empty expression', () => {
    const intoIf = buildEdgeFromConnection(
      { source: 'req', sourceHandle: 'result', target: 'iff', targetHandle: 'input' },
      req,
    );
    const trigger = buildEdgeFromConnection(
      { source: 'iff', sourceHandle: 'false', target: 'req', targetHandle: 'trigger' },
      iff,
    );
    expect(intoIf?.expression).toBe('');
    expect(trigger).toMatchObject({ expression: '', sourceHandle: 'false', targetField: 'trigger' });
  });
});

describe('isDataLessTarget / shouldPromptForExpression', () => {
  it('treats only input and trigger as data-less', () => {
    expect(['input', 'trigger', 'url', 'headers', 'body', 'value'].map(isDataLessTarget)).toEqual([
      true,
      true,
      false,
      false,
      false,
      false,
    ]);
  });

  it('prompts for an expression only on data wires', () => {
    const base: FlowEdge = {
      id: 'e',
      sourceNodeId: 'a',
      targetNodeId: 'b',
      targetField: 'url',
      expression: 'response.body',
    };
    expect(shouldPromptForExpression(base)).toBe(true);
    expect(shouldPromptForExpression({ ...base, targetField: 'trigger', expression: '' })).toBe(
      false,
    );
    expect(shouldPromptForExpression({ ...base, targetField: 'input', expression: '' })).toBe(false);
  });
});

describe('isValidFlowConnection', () => {
  const conn = (
    source: string,
    sourceHandle: string | null,
    target: string,
    targetHandle: string,
  ) => ({ source, sourceHandle, target, targetHandle });

  it('accepts plain data wires, including a null source handle from a Request', () => {
    expect(isValidFlowConnection(conn('req', null, 'out', 'value'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('inp', 'result', 'req', 'url'), nodes, [])).toBe(true);
  });

  it('accepts routing exits into data fields and Run when inputs', () => {
    expect(isValidFlowConnection(conn('iff', 'true', 'req', 'headers'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('iff', 'false', 'out', 'trigger'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('sw', 'case:c1', 'req', 'trigger'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('sw', 'default', 'out', 'value'), nodes, [])).toBe(true);
  });

  it('rejects a source handle that does not exist on the source node', () => {
    expect(isValidFlowConnection(conn('iff', null, 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('iff', 'result', 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'true', 'out', 'value'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('sw', 'case:deleted', 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('sw', 'case:', 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('out', 'result', 'req', 'url'), nodes, [])).toBe(false);
  });

  it('rejects wires into an Input node and misplaced input/trigger handles', () => {
    expect(isValidFlowConnection(conn('req', 'result', 'inp', 'value'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'result', 'req', 'input'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'result', 'iff', 'trigger'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'result', 'sw', 'url'), nodes, [])).toBe(false);
  });

  it('allows only one wire into an If or Switch input', () => {
    const existing: FlowEdge[] = [
      { id: 'e1', sourceNodeId: 'inp', targetNodeId: 'iff', targetField: 'input', expression: '' },
    ];
    expect(isValidFlowConnection(conn('req', 'result', 'iff', 'input'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('req', 'result', 'iff', 'input'), nodes, existing)).toBe(
      false,
    );
  });

  it('rejects a connection with a missing endpoint or unknown node', () => {
    expect(
      isValidFlowConnection(
        { source: null, sourceHandle: null, target: 'out', targetHandle: 'value' },
        nodes,
        [],
      ),
    ).toBe(false);
    expect(isValidFlowConnection(conn('ghost', 'result', 'out', 'value'), nodes, [])).toBe(false);
  });
});

describe('parseGraphErrorMessage', () => {
  it('extracts node and edge ids from a cycle rejection', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2';
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: ['a', 'b'], edgeIds: ['e1', 'e2'] });
  });

  it('still parses the older cycle message without an edge segment', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b';
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: ['a', 'b'], edgeIds: [] });
  });

  it('parses a node validation error whose edge list is empty', () => {
    const message =
      "Invalid input: If node 'b' must have exactly one incoming edge — node(s): b; edge(s): ";
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: ['b'], edgeIds: [] });
  });

  it('parses an edge validation error whose node list is empty', () => {
    const message = "Invalid input: edge 'e3' leaves from unknown exit 'case:gone' — node(s): ; edge(s): e3";
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: [], edgeIds: ['e3'] });
  });

  it('returns null for an unrelated error message', () => {
    expect(parseGraphErrorMessage('Invalid input: flow name is empty')).toBeNull();
  });
});
```

Delete the old `describe('parseCycleErrorMessage', …)` block. The first two new `parseGraphErrorMessage` tests replace it.

In `src/components/flow/__tests__/FlowPane.test.tsx`, inside `describe('FlowPane save', …)` and after `'flags the node ids named in a cycle rejection'`, add:

```ts
  it('flags the node named in a non-cycle validation error', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      "Invalid input: If node 'b' must have exactly one incoming edge — node(s): b; edge(s): ",
    );
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      const cards = screen.getAllByTestId('output-node-card');
      const flagged = cards.filter((c) => c.className.includes('ring-red-500'));
      expect(flagged.map((c) => c.textContent)).toEqual(['Out b—']);
    });
  });
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `yarn test --run flow-wiring FlowPane`
Expected: FAIL. `parseGraphErrorMessage`, `isValidFlowConnection`, `isDataLessTarget` and `shouldPromptForExpression` are not exported ("is not a function"). The new `FlowPane` test finds no flagged card.

- [ ] **Step 3: Rewrite `src/lib/flow-wiring.ts`**

```ts
import type { Connection } from '@xyflow/react';
import {
  caseIdFromHandle,
  DEFAULT_HANDLE,
  FALSE_HANDLE,
  INPUT_HANDLE,
  isRoutingKind,
  RESULT_HANDLE,
  TRIGGER_HANDLE,
  TRUE_HANDLE,
} from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

// Every source kind is evaluated as a response-shaped object. An Input
// node's value is its `response.body` (see resolve_flow_wire_expression).
// The node argument is kept so a later per-kind default is a local change.
export function defaultExpressionFor(_sourceNode: FlowNode): string {
  return 'response.body';
}

// An If/Switch `input` and a "Run when" `trigger` carry no wired value, so
// their edges have no expression to evaluate or edit.
export function isDataLessTarget(targetHandle: string): boolean {
  return targetHandle === INPUT_HANDLE || targetHandle === TRIGGER_HANDLE;
}

export function shouldPromptForExpression(edge: FlowEdge): boolean {
  return !isDataLessTarget(edge.targetField);
}

export function buildEdgeFromConnection(
  connection: Connection,
  sourceNode: FlowNode,
): FlowEdge | null {
  if (!connection.source || !connection.target || !connection.targetHandle) return null;
  const edge: FlowEdge = {
    id: crypto.randomUUID(),
    sourceNodeId: connection.source,
    targetNodeId: connection.target,
    targetField: connection.targetHandle,
    expression: isDataLessTarget(connection.targetHandle) ? '' : defaultExpressionFor(sourceNode),
  };
  // The backend omits `result` on disk, so leave it out here too. That keeps
  // re-saved Phase 1 flows byte-identical.
  if (connection.sourceHandle && connection.sourceHandle !== RESULT_HANDLE) {
    edge.sourceHandle = connection.sourceHandle;
  }
  return edge;
}

// React Flow passes either a Connection or an Edge to isValidConnection.
export type ConnectionLike = {
  source: string | null;
  target: string | null;
  sourceHandle?: string | null;
  targetHandle?: string | null;
};

function sourceHandleExists(node: FlowNode, handle: string): boolean {
  switch (node.kind.kind) {
    case 'Request':
    case 'Input':
      return handle === RESULT_HANDLE;
    case 'If':
      return handle === TRUE_HANDLE || handle === FALSE_HANDLE;
    case 'Switch': {
      if (handle === DEFAULT_HANDLE) return true;
      const caseId = caseIdFromHandle(handle);
      return caseId !== null && node.kind.cases.some((c) => c.id === caseId);
    }
    case 'Output':
      return false;
  }
}

const REQUEST_TARGETS = ['url', 'headers', 'body', TRIGGER_HANDLE];
const OUTPUT_TARGETS = ['value', TRIGGER_HANDLE];

function targetAccepts(node: FlowNode, handle: string): boolean {
  switch (node.kind.kind) {
    case 'If':
    case 'Switch':
      return handle === INPUT_HANDLE;
    case 'Request':
      return REQUEST_TARGETS.includes(handle);
    case 'Output':
      return OUTPUT_TARGETS.includes(handle);
    case 'Input':
      return false;
  }
}

// Client-side copy of rocket_flow::validate rules V1–V5, so obviously
// invalid wires cannot be drawn. Save still runs the real validation, and
// cycles are left to it.
export function isValidFlowConnection(
  connection: ConnectionLike,
  nodes: FlowNode[],
  edges: FlowEdge[],
): boolean {
  const { source, target, targetHandle } = connection;
  if (!source || !target || !targetHandle) return false;
  const sourceNode = nodes.find((n) => n.id === source);
  const targetNode = nodes.find((n) => n.id === target);
  if (!sourceNode || !targetNode) return false;
  if (!sourceHandleExists(sourceNode, connection.sourceHandle ?? RESULT_HANDLE)) return false;
  if (!targetAccepts(targetNode, targetHandle)) return false;
  // A routing node evaluates exactly one input.
  if (isRoutingKind(targetNode.kind) && edges.some((e) => e.targetNodeId === target)) return false;
  return true;
}

export interface GraphErrorIds {
  nodeIds: string[];
  edgeIds: string[];
}

const splitIds = (list: string | undefined): string[] =>
  (list ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);

// Every save_flow validation error ends with "node(s): a, b; edge(s): e1".
// That covers a cycle and rocket_flow::validate's InvalidNode/InvalidEdge.
// Either list may be empty. Older cycle messages have no edge segment.
export function parseGraphErrorMessage(message: string): GraphErrorIds | null {
  const match = message.match(/node\(s\): ([^;]*)(?:; edge\(s\): (.*))?$/);
  if (!match) return null;
  return { nodeIds: splitIds(match[1]), edgeIds: splitIds(match[2]) };
}
```

- [ ] **Step 4: Switch `FlowPane` to the generic parser**

In `src/components/flow/FlowPane.tsx`, change the import on line 14:

```ts
import { buildEdgeFromConnection, parseGraphErrorMessage } from '@/lib/flow-wiring';
```

In `handleSave`'s `catch` block, replace the comment and the parse call:

```ts
    } catch (err) {
      // Any validation error names the offending node(s) and edge(s) at the
      // end of the message. Flag them on the canvas as well as toasting.
      const message = String(err);
      const parsed = parseGraphErrorMessage(message);
      if (parsed) {
        setCycleNodeIds(parsed.nodeIds);
        setCycleEdgeIds(parsed.edgeIds);
      }
      toast.error(`Could not save flow: ${message}`);
      return false;
    }
```

The `cycleNodeIds`/`cycleEdgeIds` state and the `hasCycleError` data flag keep their names. Plan 05 reuses them for every validation error.

- [ ] **Step 5: Run the tests and verify they pass**

Run: `yarn test --run flow-wiring FlowPane`
Expected: PASS, all tests.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0. If Biome reports formatting only, run `yarn format` and re-run `yarn check`.

- [ ] **Step 6: Commit**

```bash
git add src/lib/flow-wiring.ts src/lib/__tests__/flow-wiring.test.ts \
  src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.test.tsx
```

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add exit-aware wiring and connection rules`.

---

### Task 3: Canvas and pane wiring, plus "Run when" handles

**Files:**
- Modify: `src/components/flow/FlowCanvas.tsx:90-107` (`toRfEdges`), and the `<ReactFlow>` props (~line 251)
- Modify: `src/components/flow/FlowPane.tsx:217-232` (`handleConnect`)
- Modify: `src/components/flow/nodes/RequestNode.tsx` (field rows, ~line 75)
- Modify: `src/components/flow/nodes/OutputNode.tsx`
- Test: `src/components/flow/__tests__/FlowCanvas.test.tsx`
- Test: `src/components/flow/nodes/__tests__/RequestNode.test.tsx:80-92`
- Test: `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx:57-80`

**Interfaces:**
- Consumes: from Task 2, `isValidFlowConnection`, `shouldPromptForExpression` and `ConnectionLike`. From Task 1, `RESULT_HANDLE` and `TRIGGER_HANDLE`.
- Produces:
  - `export function toRfEdges(edges: FlowEdge[], selectedIds: ReadonlySet<string>, cycleEdgeIds?: string[]): Edge[]` from `FlowCanvas.tsx`. Plan 05 extends it with labels and taken/not-taken styling.
  - A target handle `id="trigger"` on `RequestNode` (the first target row) and on `OutputNode`.

- [ ] **Step 1: Write the failing tests**

In `src/components/flow/__tests__/FlowCanvas.test.tsx`, change the import to `import { FlowCanvas, toRfEdges } from '../FlowCanvas';`. Add this block at the end of the outer `describe('FlowCanvas', …)`:

```ts
  describe('toRfEdges', () => {
    it('maps a missing sourceHandle to result and keeps a routing exit', () => {
      const rf = toRfEdges(
        [
          {
            id: 'e1',
            sourceNodeId: 'a',
            targetNodeId: 'b',
            targetField: 'headers[Authorization].value',
            expression: 'response.body',
          },
          {
            id: 'e2',
            sourceNodeId: 'if1',
            targetNodeId: 'b',
            targetField: 'trigger',
            expression: '',
            sourceHandle: 'true',
          },
        ],
        new Set(),
      );
      expect(rf.map((e) => [e.id, e.sourceHandle, e.targetHandle])).toEqual([
        ['e1', 'result', 'headers'],
        ['e2', 'true', 'trigger'],
      ]);
    });
  });
```

In `src/components/flow/nodes/__tests__/RequestNode.test.tsx`, update `'exposes url, headers, and body target handles plus one result source handle'`:

```ts
  it('exposes trigger, url, headers, and body target handles plus one result source handle', () => {
    renderNode({ kind: baseKind, status: 'idle' });
    const card = screen.getByTestId('request-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['trigger', 'url', 'headers', 'body']);
    expect(screen.getByText('Run when')).toBeInTheDocument();
    const sources = [...card.querySelectorAll('.react-flow__handle.source')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(sources).toEqual(['result']);
  });
```

In `src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx`, rename the `OutputNode` test and replace its assertions:

```ts
  it('renders its label with value and Run when target handles and no source handle', () => {
    // … unchanged wrap(<OutputNode … />) call …
    expect(screen.getByText('Result')).toBeInTheDocument();
    const card = screen.getByTestId('output-node-card');
    const targets = [...card.querySelectorAll('.react-flow__handle.target')].map((h) =>
      h.getAttribute('data-handleid'),
    );
    expect(targets).toEqual(['trigger', 'value']);
    expect(card.querySelectorAll('.react-flow__handle.source')).toHaveLength(0);
    expect(card.querySelector('[data-handleid="trigger"]')?.getAttribute('title')).toBe('Run when');
  });
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `yarn test --run FlowCanvas RequestNode InputOutputNodes`
Expected: FAIL. `toRfEdges` is not exported. `RequestNode` targets are `['url','headers','body']`. `OutputNode` targets are `['value']`.

- [ ] **Step 3: Export `toRfEdges`, map exits and wire `isValidConnection`**

In `src/components/flow/FlowCanvas.tsx`:
- Add the imports `import { RESULT_HANDLE } from '@/lib/flow-handles';` and `import { type ConnectionLike, isValidFlowConnection } from '@/lib/flow-wiring';`.
- Replace `toRfEdges` and the comment above it:

```ts
// An edge leaves the source exit named by `sourceHandle`. It is absent for
// the default `result` exit. The target handle is the first segment of
// `targetField`, so "headers[Authorization].value" lands on the single
// `headers` handle.
export function toRfEdges(
  edges: FlowEdge[],
  selectedIds: ReadonlySet<string>,
  cycleEdgeIds?: string[],
): Edge[] {
  return edges.map((e) => ({
    id: e.id,
    source: e.sourceNodeId,
    sourceHandle: e.sourceHandle ?? RESULT_HANDLE,
    target: e.targetNodeId,
    targetHandle: e.targetField.split('[')[0],
    selected: selectedIds.has(e.id),
    style: cycleEdgeIds?.includes(e.id) ? { stroke: '#ef4444', strokeWidth: 2 } : undefined,
  }));
}
```

- Inside `FlowCanvasInner`, directly after the `rfEdges` `useMemo`, add:

```ts
  // Refuses wires the backend would reject, while the user is still dragging.
  const isValidConnection = (connection: ConnectionLike) =>
    isValidFlowConnection(connection, nodes, edges);
```

- Add `isValidConnection={isValidConnection}` to the `<ReactFlow …>` props, directly after `onConnect={onConnect}`.

- [ ] **Step 4: Skip the expression popover for data-less wires**

In `src/components/flow/FlowPane.tsx`, extend the flow-wiring import:

```ts
import {
  buildEdgeFromConnection,
  parseGraphErrorMessage,
  shouldPromptForExpression,
} from '@/lib/flow-wiring';
```

Replace the last two statements of `handleConnect`:

```ts
    updateFlowEdges(tab.id, [...base, edge]);
    // Input and trigger wires carry no value, so there is nothing to edit.
    // Clearing the pending edge also closes a preempted popover.
    setPendingEdge(shouldPromptForExpression(edge) ? edge : null);
  };
```

- [ ] **Step 5: Add the "Run when" handles**

In `src/components/flow/nodes/RequestNode.tsx`, make this row the **first** child of the field-row container (`<div className='relative space-y-1 px-2 py-1.5'>`), above the URL row:

```tsx
        <div className='relative flex items-center gap-1.5 pl-2'>
          <Handle
            type='target'
            id='trigger'
            position={Position.Left}
            isConnectable={isConnectable}
            className='!h-2 !w-2'
          />
          <span className='text-muted-foreground'>Run when</span>
        </div>
```

Update the comment above that container. Its first sentence should read: "Every field row, including the data-less "Run when" trigger row, is always rendered, even when empty, so each target handle stays connectable."

In `src/components/flow/nodes/OutputNode.tsx`, add this handle **before** the existing `value` handle:

```tsx
      {/* Data-less "Run when" input. It sits at the top so it does not overlap `value`. */}
      <Handle
        type='target'
        id='trigger'
        title='Run when'
        position={Position.Left}
        isConnectable={isConnectable}
        style={{ top: 10 }}
        className='!h-2 !w-2'
      />
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `yarn test --run flow`
Expected: PASS for every file under `src/components/flow` and `src/lib/__tests__/flow-*`, with no failures. The existing `FlowPane` test `'flags the node ids named in a cycle rejection'` still expects `'Out a—'`. OutputNode adds no text, so that stays true.

Run: `yarn tsc --noEmit && yarn check`
Expected: exit code 0.

- [ ] **Step 7: Commit**

```bash
git add src/components/flow/FlowCanvas.tsx src/components/flow/FlowPane.tsx \
  src/components/flow/nodes/RequestNode.tsx src/components/flow/nodes/OutputNode.tsx \
  src/components/flow/__tests__/FlowCanvas.test.tsx \
  src/components/flow/nodes/__tests__/RequestNode.test.tsx \
  src/components/flow/nodes/__tests__/InputOutputNodes.test.tsx
```

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): wire routing exits and Run when inputs on canvas`.

---

### Task 4: Forward skip reason and branch from runs

**Files:**
- Modify: `src/components/flow/FlowToolbar.tsx` (the three detail mappings, at ~lines 86-91, 137-142 and 158-163)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx`

**Interfaces:**
- Consumes: from Task 1, `FlowStepCompletedEvent.skip_reason`/`branch`, `FlowStepResult.skipReason`/`branch`, and `FlowNodeDetail`.
- Produces: `onPatchStatus(nodeId, status, detail)`, where `detail` includes `skipReason` and `branch` whenever the backend sent them. Plan 05's visualization reads these from `FlowTab.nodeDetail`.

- [ ] **Step 1: Write the failing tests**

Add these tests to `describe('FlowToolbar', …)` in `src/components/flow/__tests__/FlowToolbar.test.tsx`, after `'applies the returned summary steps as the final state'`:

```ts
  it('forwards skip_reason and branch from flow-step-completed', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'if1',
      status: 'success',
      status_code: null,
      duration_ms: null,
      error: null,
      value: null,
      branch: 'false',
    });
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'n2',
      status: 'skipped',
      status_code: null,
      duration_ms: null,
      error: null,
      value: null,
      skip_reason: 'branch_not_taken',
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'if1',
      'success',
      expect.objectContaining({ branch: 'false', skipReason: undefined }),
    );
    expect(onPatchStatus).toHaveBeenCalledWith(
      'n2',
      'skipped',
      expect.objectContaining({ skipReason: 'branch_not_taken', branch: undefined }),
    );
  });

  it('applies skipReason and branch from the returned summary', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    resolveRun({
      runId: 'run-123',
      steps: [
        {
          nodeId: 'sw1',
          status: 'success',
          statusCode: null,
          durationMs: null,
          error: null,
          value: null,
          branch: 'case:c1',
        },
        {
          nodeId: 'n3',
          status: 'skipped',
          statusCode: null,
          durationMs: null,
          error: null,
          value: null,
          skipReason: 'upstream_failed',
        },
      ],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenCalledWith(
        'n3',
        'skipped',
        expect.objectContaining({ skipReason: 'upstream_failed' }),
      ),
    );
    expect(onPatchStatus).toHaveBeenCalledWith(
      'sw1',
      'success',
      expect.objectContaining({ branch: 'case:c1' }),
    );
  });
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `yarn test --run FlowToolbar`
Expected: FAIL. `onPatchStatus` is called with a detail that has no `branch` or `skipReason` keys, so `expect.objectContaining({ branch: 'false', … })` does not match.

- [ ] **Step 3: Centralise the detail mapping in `FlowToolbar.tsx`**

Extend the tauri-api import with the types:

```ts
import {
  cancelFlowRun,
  type FlowStepCompletedEvent,
  type FlowStepResult,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
```

Directly above `export function FlowToolbar(`, add:

```ts
// Maps a streamed step event (snake_case) to the per-node detail the tab stores.
function detailFromEvent(event: FlowStepCompletedEvent): FlowNodeDetail {
  return {
    statusCode: event.status_code ?? undefined,
    durationMs: event.duration_ms ?? undefined,
    error: event.error ?? undefined,
    value: event.value ?? undefined,
    skipReason: event.skip_reason ?? undefined,
    branch: event.branch ?? undefined,
  };
}

// Maps a run_flow summary step (camelCase) to the same detail shape.
function detailFromStep(step: FlowStepResult): FlowNodeDetail {
  return {
    statusCode: step.statusCode ?? undefined,
    durationMs: step.durationMs ?? undefined,
    error: step.error ?? undefined,
    value: step.value ?? undefined,
    skipReason: step.skipReason ?? undefined,
    branch: step.branch ?? undefined,
  };
}
```

Replace the three inline object literals with these helpers.

The resumed-run subscription becomes:

```ts
    void onFlowStepCompleted((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, event.status, detailFromEvent(event));
    }).then((fn) => {
```

`handleRun`'s step subscription becomes:

```ts
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, event.status, detailFromEvent(event));
    });
```

The summary loop becomes:

```ts
      for (const step of summary.steps) {
        onPatchStatus(step.nodeId, step.status, detailFromStep(step));
      }
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `yarn test --run FlowToolbar`
Expected: PASS. The existing `'applies the returned summary steps as the final state'` test still passes, because `toHaveBeenCalledWith` ignores keys whose value is `undefined`.

Run: `yarn tsc --noEmit && yarn check && yarn test --run flow pane-store`
Expected: exit code 0, and every test passes.

- [ ] **Step 5: Commit**

```bash
git add src/components/flow/FlowToolbar.tsx src/components/flow/__tests__/FlowToolbar.test.tsx
```

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): forward skip reason and branch from runs`.

---

## Plan verification

Run all of these from the worktree root. Each must exit 0:

```bash
yarn tsc --noEmit
yarn check
yarn test --run flow pane-store
```

Manual check (optional, needs plans 01–03): `yarn tauri dev`, then open a Flow tab.
- Dragging from a Request node's `result` onto another Request node's "Run when" row creates an edge **without** opening the expression popover.
- Dragging a second wire onto the same target handle is still allowed for Request fields.

## Next Plan

[Plan 05 — Routing nodes and run visualization](2026-09-28-flow-phase2-branching-plan-05-routing-nodes-and-visualization.md)

## Post-Implementation Review

Before starting plan 05, dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review everything this plan added or modified. Its scope is `src/lib/flow-handles.ts`, `src/lib/flow-wiring.ts`, `FlowCanvas.tsx`, `FlowPane.tsx`, `FlowToolbar.tsx`, `RequestNode.tsx`, `OutputNode.tsx`, `pane-types.ts`, `pane-store.ts`, `tauri-api.ts` and their tests. It checks for:
- Handle strings drifting from `rocket_flow::handle`.
- `isValidFlowConnection` disagreeing with spec §7 V1–V5.
- Any place that still hard-codes `'result'` or builds a run-detail object inline.
- shadcn, lucide and Zustand rule violations.

The reviewer has explicit authority to fix what it finds directly (not just report it) before plan 05 starts. It commits its fixes through the `dev-workflow-skills:1-git-commit` skill.
