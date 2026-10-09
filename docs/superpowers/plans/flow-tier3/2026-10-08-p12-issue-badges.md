# Flow Issue Badges (Client Side) Implementation Plan

> **Execute this plan:** P12. Before starting it, make sure these are merged to main: none (independent; it can run at any point in the order). After it is merged, the next plan to execute is P13. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show problems in a flow while the user edits it: a severity ring and a small icon badge on each affected node, and an issue count next to Run that opens a list where a click selects the node. Errors never block a run.

**Architecture:** A pure function `computeFlowIssues(nodes, edges, ctx)` in `src/lib/flow-issues.ts` returns a list of `FlowIssue` objects (the same shape the backend lint feed of plan P21 will use). The save-error ids that `FlowPane` already tracks are folded in as `code: 'save'` errors. `FlowPane` computes the list with `useMemo` and passes it to `FlowCanvas`, which puts each node's issues in the node `data`, replacing the old `hasCycleError` flag. All eight node components draw the same ring and the same `NodeIssueBadge`. A new `FlowIssuesButton` (shadcn `Popover`) lists every issue. Frontend only.

**Tech Stack:** React, TypeScript, `@xyflow/react` 12.12.0, Vitest and Testing Library, shadcn `Popover`, `Tooltip` and `Button`, lucide-react.

**Spec:** Roadmap item F-37 (client part, F-37a) in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P12) and the plan index (`00-plan-index.md`, plan P12).

**Independent of P1 and P2.** This plan touches `FlowPane.tsx` and `FlowCanvas.tsx` but none of the code those plans add. If P1 or P2 has merged first, only the surrounding JSX differs; every anchor below is a prop or function name.

**Findings from checking the code (design notes were slightly off):**
- All eight node components draw the ring through `data.hasCycleError && 'ring-2 ring-red-500'`: `AuthNode`, `IfNode`, `InputNode`, `OutputNode`, `RequestNode`, `SwitchNode`, `TransformNode`, `WaitForCallbackNode` (all in `src/components/flow/nodes/`). There is no ninth.
- The existing tests `FlowPane.test.tsx` (the three "flags the node ..." tests) and `RequestNode.test.tsx` (`outlines the card when it is part of a rejected cycle`) depend on the red ring, and the FlowPane ones compare each card's `textContent` (for example `'Out aRun whenValue—'`). The badge therefore must add **no text**, only an `aria-label` and an svg icon. The three FlowPane tests keep passing because an Output with no value wire is a **warning** (amber ring), not an error.
- Cycle edges stay red through the existing `cycleEdgeIds` prop and `toRfEdges`. This plan changes node data only.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No Rust changes in this plan. The backend lint feed is plan P21; keep the `FlowIssue` shape stable.
- No `backdrop-filter` or `color-mix` in new styles (they hang WebKitGTK paint on this machine). Use plain Tailwind colour utilities.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check` (if it only reports import order or formatting, run `yarn lint` and `yarn format`, review the diff, and re-check), and the targeted `yarn test <pattern>` listed in the task.
- Only one implementer at a time touches `FlowPane.tsx`, `FlowCanvas.tsx` and the node components.
- Not in scope: backend lint (P21), blocking a run on an error, edge badges (edges keep the `cycleEdgeIds` red stroke), quick-fix actions.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. The old save-error highlight must keep working: exactly the nodes named in a rejected save get a red ring, the panel still shows `saveError`, and ids of nodes deleted since the save raise no ghost issue. Tests pinned in Task 1 (folding and filtering) and Task 2 (existing `FlowPane.test.tsx` cases stay green).
2. A badge must add no text to a node card, or the `textContent` assertions in existing tests break. Test pinned in Task 2 (`NodeIssueBadge`).
3. A warning must never look like an error: amber ring, warning icon, and no `ring-red-500`. Test pinned in Task 2 (canvas, all eight node kinds).
4. Rule false positives: an inline Request with a wired `url` and no typed URL is fine; a Switch default or an Output without a value wire is only a warning; "no path to an Output" is silent when the flow has no Output at all. Tests pinned in Task 1.
5. The list must agree with the nodes: the count equals the number of issues, an item selects its node and opens its panel, and an issue with no node (edge only) is not clickable. Tests pinned in Task 3.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/flow-issues.ts` (new) | `FlowIssue` type, `computeFlowIssues`, grouping and wording helpers, `cleanSaveMessage`. |
| `src/components/flow/nodes/NodeIssueBadge.tsx` (new) | Icon-only badge with a tooltip, shared by all eight node components. |
| `src/components/flow/nodes/nodeStatus.ts` (modify) | `issueRingClassName(issues)`. |
| `src/components/flow/nodes/*Node.tsx` (modify, 8 files) | Replace `hasCycleError` with `issues`; draw the ring and the badge. |
| `src/components/flow/FlowCanvas.tsx` (modify) | `issues` prop replaces `cycleNodeIds`; `toRfNodes` puts each node's issues in its data. |
| `src/components/flow/FlowIssuesButton.tsx` (new) | Count button and popover list. |
| `src/components/flow/FlowPane.tsx` (modify) | Computes `issues`, passes them down, renders the button, selects a node from the list. |

Existing tests to know: `src/components/flow/__tests__/FlowPane.test.tsx` (the three red-ring tests around lines 120-190), `src/components/flow/__tests__/FlowPane.properties.test.tsx` (the `Harness` and jsdom polyfills), `src/components/flow/__tests__/FlowCanvas.test.tsx`, `src/components/flow/nodes/__tests__/RequestNode.test.tsx` (the `hasCycleError` test), `src/components/flow/nodes/__tests__/nodeStatus.test.ts`.

---

### Task 1: `flow-issues.ts` rules

**Files:**
- Create: `src/lib/flow-issues.ts`
- Create: `src/lib/__tests__/flow-issues.test.ts`

**Interfaces:**
- Produces:

```ts
type IssueSeverity = 'error' | 'warning';
interface FlowIssue { code: string; severity: IssueSeverity; nodeId?: string; edgeId?: string; message: string; hint?: string }
interface SaveErrorInfo { nodeIds: string[]; edgeIds: string[]; message: string | null }
interface FlowIssueContext { save?: SaveErrorInfo }
computeFlowIssues(nodes: FlowNode[], edges: FlowEdge[], ctx?: FlowIssueContext): FlowIssue[]
groupIssuesByNode(issues: FlowIssue[]): Map<string, FlowIssue[]>
worstSeverity(issues: FlowIssue[]): IssueSeverity | null
summarizeIssues(issues: FlowIssue[]): string
issueCountLabel(issues: FlowIssue[]): string
cleanSaveMessage(message: string): string
```

Issue codes: `expr-blank` (V8), `input-missing` (V1), `request-path-empty`, `request-url-empty`, `switch-duplicate-match` (V7), `wait-name-invalid`, `wait-name-duplicate`, `wait-timeout-range`, `wait-accept-empty` (all V10), `repeat-limits` (V9) as errors; `output-no-value`, `exit-unwired`, `no-path-to-output` as warnings; `save` for the folded-in save error.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-issues.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind, RepeatUntil } from '@/lib/tauri-api';
import {
  cleanSaveMessage,
  computeFlowIssues,
  type FlowIssue,
  groupIssuesByNode,
  issueCountLabel,
  summarizeIssues,
  worstSeverity,
} from '../flow-issues';

const node = (id: string, kind: FlowNodeKind): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const wire = (
  id: string,
  source: string,
  target: string,
  field: string,
  sourceHandle?: string,
): FlowEdge => ({
  id,
  sourceNodeId: source,
  targetNodeId: target,
  targetField: field,
  expression: '',
  ...(sourceHandle ? { sourceHandle } : {}),
});

const input = (id: string) => node(id, { kind: 'Input', label: id, value: 'x' });
const output = (id: string) => node(id, { kind: 'Output', label: id });
const inlineRequest = (id: string, url: string, repeatUntil?: RepeatUntil) =>
  node(id, {
    kind: 'Request',
    label: id,
    source: { type: 'Inline', request: { method: 'GET', url, headers: [] } },
    ...(repeatUntil ? { repeatUntil } : {}),
  });

const only = (issues: FlowIssue[], code: string) => issues.filter((i) => i.code === code);

describe('computeFlowIssues: a healthy flow', () => {
  it('reports nothing for wired nodes with all fields filled in', () => {
    const nodes = [
      input('in1'),
      node('tf1', { kind: 'Transform', label: 'tf1', script: 'return 1;' }),
      output('out1'),
      inlineRequest('req1', 'https://x.test'),
      output('out2'),
    ];
    const edges = [
      wire('e1', 'in1', 'tf1', 'input'),
      wire('e2', 'tf1', 'out1', 'value'),
      wire('e3', 'req1', 'out2', 'value'),
    ];
    expect(computeFlowIssues(nodes, edges)).toEqual([]);
  });

  it('reports nothing for an empty flow', () => {
    expect(computeFlowIssues([], [])).toEqual([]);
  });
});

describe('computeFlowIssues: expressions and inputs', () => {
  it.each<[string, FlowNodeKind]>([
    ['If', { kind: 'If', label: 'c', condition: '  ' }],
    ['Switch', { kind: 'Switch', label: 'c', value: '', cases: [] }],
    ['Transform', { kind: 'Transform', label: 'c', script: '\n' }],
  ])('flags a blank %s expression as an error', (_name, kind) => {
    const issues = computeFlowIssues([input('in1'), node('n1', kind)], [wire('e1', 'in1', 'n1', 'input')]);
    const blank = only(issues, 'expr-blank');
    expect(blank).toHaveLength(1);
    expect(blank[0]).toMatchObject({ severity: 'error', nodeId: 'n1' });
  });

  it('flags an If, Switch or Transform with no input wire, and only then', () => {
    const nodes = [node('n1', { kind: 'If', label: 'c', condition: 'true' }), input('in1')];
    expect(only(computeFlowIssues(nodes, []), 'input-missing').map((i) => i.nodeId)).toEqual(['n1']);
    expect(only(computeFlowIssues(nodes, [wire('e1', 'in1', 'n1', 'input')]), 'input-missing')).toEqual([]);
  });

  it('warns for an Output without a value wire, even with a Run when wire', () => {
    const nodes = [input('in1'), output('out1')];
    const trigger = [wire('e1', 'in1', 'out1', 'trigger')];
    const warned = only(computeFlowIssues(nodes, trigger), 'output-no-value');
    expect(warned).toHaveLength(1);
    expect(warned[0]).toMatchObject({ severity: 'warning', nodeId: 'out1' });
    expect(only(computeFlowIssues(nodes, [wire('e2', 'in1', 'out1', 'value')]), 'output-no-value')).toEqual([]);
  });
});

describe('computeFlowIssues: Request nodes', () => {
  it('flags an empty saved-request path', () => {
    const saved = node('r1', { kind: 'Request', label: 'r1', source: { type: 'Saved', requestPath: ' ' } });
    expect(only(computeFlowIssues([saved], []), 'request-path-empty')[0]).toMatchObject({
      severity: 'error',
      nodeId: 'r1',
    });
  });

  it('flags an empty inline URL unless a wire feeds the url field', () => {
    const req = inlineRequest('r1', '');
    expect(only(computeFlowIssues([req], []), 'request-url-empty')).toHaveLength(1);
    const wired = computeFlowIssues([input('in1'), req], [wire('e1', 'in1', 'r1', 'url')]);
    expect(only(wired, 'request-url-empty')).toEqual([]);
  });

  it.each<[string, Partial<RepeatUntil>, string]>([
    ['blank condition', { condition: ' ' }, 'condition is empty'],
    ['short interval', { intervalMs: 50 }, 'at least 100 ms'],
    ['zero attempts', { maxAttempts: 0 }, 'between 1 and 1000'],
    ['too many attempts', { maxAttempts: 1001 }, 'between 1 and 1000'],
    ['huge timeout', { timeoutMs: 3_600_001 }, 'at most 3600000 ms'],
    ['timeout under interval', { intervalMs: 5000, timeoutMs: 4000 }, 'shorter than the interval'],
  ])('flags repeat-until %s', (_name, patch, fragment) => {
    const base: RepeatUntil = { condition: 'response.status === 200', intervalMs: 1000, maxAttempts: 10, timeoutMs: 60000 };
    const issues = computeFlowIssues([inlineRequest('r1', 'https://x.test', { ...base, ...patch })], []);
    const found = only(issues, 'repeat-limits');
    expect(found).toHaveLength(1);
    expect(found[0].message).toContain(fragment);
  });

  it('accepts the default repeat-until settings', () => {
    const base: RepeatUntil = { condition: 'response.status === 200', intervalMs: 2000, maxAttempts: 30, timeoutMs: 60000 };
    expect(only(computeFlowIssues([inlineRequest('r1', 'https://x.test', base)], []), 'repeat-limits')).toEqual([]);
  });
});

describe('computeFlowIssues: Switch and Wait nodes', () => {
  it('flags duplicate case matches', () => {
    const sw = node('s1', {
      kind: 'Switch',
      label: 's1',
      value: 'x',
      cases: [
        { id: 'c1', label: 'A', matches: 'a' },
        { id: 'c2', label: 'B', matches: 'a' },
      ],
    });
    const issues = computeFlowIssues([input('in1'), sw], [wire('e1', 'in1', 's1', 'input')]);
    expect(only(issues, 'switch-duplicate-match')).toHaveLength(1);
  });

  const wait = (id: string, name: string, over: Partial<Extract<FlowNodeKind, { kind: 'WaitForCallback' }>> = {}) =>
    node(id, { kind: 'WaitForCallback', label: id, name, timeoutMs: 60000, ...over });

  it('flags an empty or invalid Wait name', () => {
    expect(only(computeFlowIssues([wait('w1', '')], []), 'wait-name-invalid')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'my hook')], []), 'wait-name-invalid')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'my_hook')], []), 'wait-name-invalid')).toEqual([]);
  });

  it('flags every Wait node that shares a name', () => {
    const issues = computeFlowIssues([wait('w1', 'cb'), wait('w2', 'cb'), wait('w3', 'other')], []);
    expect(only(issues, 'wait-name-duplicate').map((i) => i.nodeId)).toEqual(['w1', 'w2']);
  });

  it('flags a Wait timeout outside 1 s to 1 h and a blank accept_when', () => {
    expect(only(computeFlowIssues([wait('w1', 'cb', { timeoutMs: 999 })], []), 'wait-timeout-range')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'cb', { timeoutMs: 3_600_001 })], []), 'wait-timeout-range')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'cb', { acceptWhen: ' ' })], []), 'wait-accept-empty')).toHaveLength(1);
    expect(only(computeFlowIssues([wait('w1', 'cb', { acceptWhen: null })], []), 'wait-accept-empty')).toEqual([]);
  });
});

describe('computeFlowIssues: warnings about the shape of the flow', () => {
  const ifNode = node('if1', { kind: 'If', label: 'if1', condition: 'true' });
  const base = [input('in1'), ifNode, output('out1'), output('out2')];

  it('warns about unwired If exits, once per node', () => {
    const edges = [
      wire('e1', 'in1', 'if1', 'input'),
      wire('e2', 'if1', 'out1', 'value', 'true'),
      wire('e3', 'in1', 'out2', 'value'),
    ];
    const unwired = only(computeFlowIssues(base, edges), 'exit-unwired');
    expect(unwired).toHaveLength(1);
    expect(unwired[0]).toMatchObject({ severity: 'warning', nodeId: 'if1' });
    expect(unwired[0].message).toContain('false');
    expect(unwired[0].message).not.toContain('true');
  });

  it('is silent when both If exits are wired', () => {
    const edges = [
      wire('e1', 'in1', 'if1', 'input'),
      wire('e2', 'if1', 'out1', 'value', 'true'),
      wire('e3', 'if1', 'out2', 'value', 'false'),
    ];
    expect(only(computeFlowIssues(base, edges), 'exit-unwired')).toEqual([]);
  });

  it('warns about an unwired Switch default and unwired cases', () => {
    const sw = node('s1', {
      kind: 'Switch',
      label: 's1',
      value: 'x',
      cases: [{ id: 'c1', label: '', matches: 'a' }],
    });
    const edges = [wire('e1', 'in1', 's1', 'input')];
    const unwired = only(computeFlowIssues([input('in1'), sw, output('out1')], edges), 'exit-unwired');
    expect(unwired).toHaveLength(1);
    expect(unwired[0].message).toContain('Case 1');
    expect(unwired[0].message).toContain('default');
  });

  it('warns about a node that leads to no Output, but only when the flow has an Output', () => {
    const lonely = [input('in1'), output('out1'), input('in2')];
    const edges = [wire('e1', 'in1', 'out1', 'value')];
    const warned = only(computeFlowIssues(lonely, edges), 'no-path-to-output');
    expect(warned.map((i) => i.nodeId)).toEqual(['in2']);
    expect(only(computeFlowIssues([input('in1'), input('in2')], []), 'no-path-to-output')).toEqual([]);
  });

  it('counts a path through an Auth node and a Request as reaching the Output', () => {
    const auth = node('a1', {
      kind: 'Auth',
      label: 'a1',
      auth: { authType: 'bearer', token: 't' },
      applyToInherit: false,
    });
    const nodes = [auth, inlineRequest('r1', 'https://x.test'), output('out1')];
    const edges = [wire('e1', 'a1', 'r1', 'auth'), wire('e2', 'r1', 'out1', 'value')];
    expect(only(computeFlowIssues(nodes, edges), 'no-path-to-output')).toEqual([]);
  });
});

describe('computeFlowIssues: save errors', () => {
  const nodes = [output('a'), output('b')];

  it('folds the nodes and wires of a rejected save in as errors', () => {
    const issues = computeFlowIssues(nodes, [], {
      save: { nodeIds: ['b'], edgeIds: [], message: 'Invalid input: flow contains a cycle through node(s): b; edge(s): ' },
    });
    const saved = only(issues, 'save');
    expect(saved).toEqual([
      expect.objectContaining({ severity: 'error', nodeId: 'b', message: 'Flow contains a cycle.' }),
    ]);
  });

  it('drops ids of nodes and wires that no longer exist', () => {
    const edges = [wire('e1', 'a', 'b', 'value')];
    const issues = computeFlowIssues(nodes, edges, {
      save: { nodeIds: ['gone', 'a'], edgeIds: ['e1', 'e-gone'], message: null },
    });
    const saved = only(issues, 'save');
    expect(saved.map((i) => i.nodeId ?? i.edgeId)).toEqual(['a', 'e1']);
    expect(saved.every((i) => i.severity === 'error')).toBe(true);
  });

  it('lists errors before warnings', () => {
    const issues = computeFlowIssues(nodes, [], { save: { nodeIds: ['b'], edgeIds: [], message: null } });
    const severities = issues.map((i) => i.severity);
    expect(severities).toEqual([...severities].sort((x, y) => (x === y ? 0 : x === 'error' ? -1 : 1)));
    expect(severities[0]).toBe('error');
  });
});

describe('cleanSaveMessage', () => {
  it('strips the prefix and the id list and ends with a full stop', () => {
    expect(
      cleanSaveMessage('Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2'),
    ).toBe('Flow contains a cycle.');
    expect(
      cleanSaveMessage(
        'Invalid input: flow is invalid: the If node needs exactly one input wire, found 0 — node(s): b; edge(s): ',
      ),
    ).toBe('The If node needs exactly one input wire, found 0.');
  });

  it('leaves a message without ids readable', () => {
    expect(cleanSaveMessage('disk full')).toBe('Disk full.');
  });
});

describe('issue helpers', () => {
  const error: FlowIssue = { code: 'expr-blank', severity: 'error', nodeId: 'n1', message: 'Boom.' };
  const warning: FlowIssue = { code: 'exit-unwired', severity: 'warning', nodeId: 'n1', message: 'Careful.' };
  const other: FlowIssue = { code: 'save', severity: 'error', edgeId: 'e1', message: 'Bad wire.' };

  it('groups by node and skips edge-only issues', () => {
    const grouped = groupIssuesByNode([error, warning, other]);
    expect(grouped.get('n1')).toEqual([error, warning]);
    expect(grouped.size).toBe(1);
  });

  it('reports the worst severity', () => {
    expect(worstSeverity([])).toBeNull();
    expect(worstSeverity([warning])).toBe('warning');
    expect(worstSeverity([warning, error])).toBe('error');
  });

  it('summarises one issue or several', () => {
    expect(summarizeIssues([error])).toBe('Error: Boom.');
    expect(summarizeIssues([error, warning])).toBe('2 issues: Error: Boom. Warning: Careful.');
  });

  it('counts errors and warnings in words', () => {
    expect(issueCountLabel([])).toBe('No issues');
    expect(issueCountLabel([error])).toBe('1 error');
    expect(issueCountLabel([error, other, warning])).toBe('2 errors, 1 warning');
    expect(issueCountLabel([warning, warning])).toBe('2 warnings');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-issues.test.ts`
Expected: FAIL, cannot resolve `../flow-issues`.

- [ ] **Step 3: Write the rules**

Create `src/lib/flow-issues.ts`:

```ts
import {
  isValidCallbackName,
  MAX_CALLBACK_TIMEOUT_MS,
  MIN_CALLBACK_TIMEOUT_MS,
} from '@/lib/flow-callback';
import {
  caseHandle,
  DEFAULT_HANDLE,
  FALSE_HANDLE,
  RESULT_HANDLE,
  TRUE_HANDLE,
  takesSingleInput,
} from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeKind, RepeatUntil } from '@/lib/tauri-api';

export type IssueSeverity = 'error' | 'warning';

// The same shape the backend lint feed (plan P21) will use, so the two merge by
// `(code, nodeId)` later.
export interface FlowIssue {
  code: string;
  severity: IssueSeverity;
  nodeId?: string;
  edgeId?: string;
  message: string;
  hint?: string;
}

// What the last rejected save named. `message` is the full error text.
export interface SaveErrorInfo {
  nodeIds: string[];
  edgeIds: string[];
  message: string | null;
}

export interface FlowIssueContext {
  save?: SaveErrorInfo;
}

// These limits must match rocket_flow::node (RepeatUntil) and flow-callback.ts.
const REPEAT_MIN_INTERVAL_MS = 100;
const REPEAT_MAX_ATTEMPTS = 1000;
const REPEAT_MAX_TIMEOUT_MS = 3_600_000;

const KIND_NAMES: Record<FlowNodeKind['kind'], string> = {
  Request: 'Request',
  Input: 'Input',
  Output: 'Output',
  If: 'If',
  Switch: 'Switch',
  WaitForCallback: 'Wait for callback',
  Transform: 'Transform',
  Auth: 'Auth',
};

function issue(
  code: string,
  severity: IssueSeverity,
  node: FlowNode,
  message: string,
  hint?: string,
): FlowIssue {
  return { code, severity, nodeId: node.id, message, ...(hint ? { hint } : {}) };
}

const hasWire = (edges: FlowEdge[], nodeId: string, field: string) =>
  edges.some((e) => e.targetNodeId === nodeId && e.targetField === field);

function expressionIssues(node: FlowNode): FlowIssue[] {
  const { kind } = node;
  let text: string | null = null;
  let field = '';
  if (kind.kind === 'If') {
    text = kind.condition;
    field = 'condition';
  } else if (kind.kind === 'Switch') {
    text = kind.value;
    field = 'value';
  } else if (kind.kind === 'Transform') {
    text = kind.script;
    field = 'script';
  }
  if (text === null || text.trim() !== '') return [];
  return [
    issue(
      'expr-blank',
      'error',
      node,
      `The ${KIND_NAMES[kind.kind]} node's ${field} is empty.`,
      'Open the node and enter an expression.',
    ),
  ];
}

function inputIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  if (!takesSingleInput(node.kind)) return [];
  if (edges.some((e) => e.targetNodeId === node.id)) return [];
  return [
    issue(
      'input-missing',
      'error',
      node,
      `The ${KIND_NAMES[node.kind.kind]} node needs an input wire.`,
      'Drag a wire from another node into its input.',
    ),
  ];
}

function outputIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  if (node.kind.kind !== 'Output' || hasWire(edges, node.id, 'value')) return [];
  return [
    issue(
      'output-no-value',
      'warning',
      node,
      'No value is wired into this Output.',
      'Wire a node into the value field to show a result.',
    ),
  ];
}

function repeatReason(r: RepeatUntil): string | null {
  if (!r.condition.trim()) return 'The repeat-until condition is empty.';
  if (r.intervalMs < REPEAT_MIN_INTERVAL_MS) {
    return `The repeat-until interval must be at least ${REPEAT_MIN_INTERVAL_MS} ms.`;
  }
  if (r.maxAttempts < 1 || r.maxAttempts > REPEAT_MAX_ATTEMPTS) {
    return `Repeat-until max attempts must be between 1 and ${REPEAT_MAX_ATTEMPTS}.`;
  }
  if (r.timeoutMs > REPEAT_MAX_TIMEOUT_MS) {
    return `The repeat-until timeout must be at most ${REPEAT_MAX_TIMEOUT_MS} ms.`;
  }
  if (r.timeoutMs < r.intervalMs) {
    return 'The repeat-until timeout must not be shorter than the interval.';
  }
  return null;
}

function requestIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  const { kind } = node;
  if (kind.kind !== 'Request') return [];
  const out: FlowIssue[] = [];
  if (kind.source.type === 'Saved') {
    if (!kind.source.requestPath.trim()) {
      out.push(
        issue(
          'request-path-empty',
          'error',
          node,
          'No saved request is chosen.',
          'Pick a request in the node settings.',
        ),
      );
    }
  } else if (!kind.source.request.url.trim() && !hasWire(edges, node.id, 'url')) {
    out.push(
      issue(
        'request-url-empty',
        'error',
        node,
        'The request URL is empty.',
        'Type a URL or wire one into the url field.',
      ),
    );
  }
  const reason = kind.repeatUntil ? repeatReason(kind.repeatUntil) : null;
  if (reason) out.push(issue('repeat-limits', 'error', node, reason));
  return out;
}

function switchIssues(node: FlowNode): FlowIssue[] {
  if (node.kind.kind !== 'Switch') return [];
  const seen = new Set<string>();
  for (const c of node.kind.cases) {
    if (seen.has(c.matches)) {
      return [
        issue(
          'switch-duplicate-match',
          'error',
          node,
          `More than one case matches '${c.matches}'.`,
          'Give each case a different match value.',
        ),
      ];
    }
    seen.add(c.matches);
  }
  return [];
}

function waitIssues(node: FlowNode, nodes: FlowNode[]): FlowIssue[] {
  const { kind } = node;
  if (kind.kind !== 'WaitForCallback') return [];
  const out: FlowIssue[] = [];
  if (!isValidCallbackName(kind.name)) {
    out.push(
      issue(
        'wait-name-invalid',
        'error',
        node,
        'The Wait for callback name must use only letters, digits and _.',
      ),
    );
  } else if (
    nodes.some(
      (n) => n.id !== node.id && n.kind.kind === 'WaitForCallback' && n.kind.name === kind.name,
    )
  ) {
    out.push(
      issue(
        'wait-name-duplicate',
        'error',
        node,
        `More than one Wait for callback node is named '${kind.name}'.`,
        'Give each Wait node its own name.',
      ),
    );
  }
  if (kind.timeoutMs < MIN_CALLBACK_TIMEOUT_MS || kind.timeoutMs > MAX_CALLBACK_TIMEOUT_MS) {
    out.push(
      issue(
        'wait-timeout-range',
        'error',
        node,
        `The Wait for callback timeout must be between ${MIN_CALLBACK_TIMEOUT_MS} and ${MAX_CALLBACK_TIMEOUT_MS} ms.`,
      ),
    );
  }
  if (kind.acceptWhen != null && kind.acceptWhen.trim() === '') {
    out.push(
      issue(
        'wait-accept-empty',
        'error',
        node,
        "The Wait for callback node's accept condition is empty.",
        'Clear the field to accept the first call, or enter a condition.',
      ),
    );
  }
  return out;
}

function exitIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  const { kind } = node;
  let exits: { handle: string; name: string }[];
  if (kind.kind === 'If') {
    exits = [
      { handle: TRUE_HANDLE, name: 'true' },
      { handle: FALSE_HANDLE, name: 'false' },
    ];
  } else if (kind.kind === 'Switch') {
    exits = [
      ...kind.cases.map((c, i) => ({
        handle: caseHandle(c.id),
        name: c.label.trim() || `Case ${i + 1}`,
      })),
      { handle: DEFAULT_HANDLE, name: 'default' },
    ];
  } else {
    return [];
  }
  const unwired = exits
    .filter(
      (x) =>
        !edges.some(
          (e) => e.sourceNodeId === node.id && (e.sourceHandle ?? RESULT_HANDLE) === x.handle,
        ),
    )
    .map((x) => x.name);
  if (unwired.length === 0) return [];
  return [
    issue(
      'exit-unwired',
      'warning',
      node,
      `Nothing is wired to the ${unwired.join(', ')} exit${unwired.length > 1 ? 's' : ''}.`,
      'A run that takes an unwired exit ends that branch.',
    ),
  ];
}

// Ids of every node with a path to an Output, or null when the flow has no Output.
function nodesReachingOutput(nodes: FlowNode[], edges: FlowEdge[]): Set<string> | null {
  const outputs = nodes.filter((n) => n.kind.kind === 'Output').map((n) => n.id);
  if (outputs.length === 0) return null;
  const sourcesOf = new Map<string, string[]>();
  for (const e of edges) {
    sourcesOf.set(e.targetNodeId, [...(sourcesOf.get(e.targetNodeId) ?? []), e.sourceNodeId]);
  }
  const reached = new Set(outputs);
  const stack = [...outputs];
  while (stack.length > 0) {
    const id = stack.pop();
    if (id === undefined) break;
    for (const source of sourcesOf.get(id) ?? []) {
      if (!reached.has(source)) {
        reached.add(source);
        stack.push(source);
      }
    }
  }
  return reached;
}

// Turns a save_flow error such as
// "Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1"
// into a sentence a person can read, without ids.
export function cleanSaveMessage(message: string): string {
  const cleaned = message
    .replace(/^Invalid input:\s*/, '')
    .replace(/^flow is invalid:\s*/, '')
    .replace(/\s*(?:—|-)?\s*node\(s\):[^;]*(?:;\s*edge\(s\):.*)?$/, '')
    .replace(/\s+through\s*$/, '')
    .trim();
  if (!cleaned) return 'The last save was rejected.';
  const sentence = cleaned.charAt(0).toUpperCase() + cleaned.slice(1);
  return /[.!?]$/.test(sentence) ? sentence : `${sentence}.`;
}

function saveIssues(save: SaveErrorInfo, nodeIds: Set<string>, edgeIds: Set<string>): FlowIssue[] {
  const message = cleanSaveMessage(save.message ?? '');
  const hint = 'Fix this, then save again.';
  return [
    ...save.nodeIds
      .filter((id) => nodeIds.has(id))
      .map((nodeId): FlowIssue => ({ code: 'save', severity: 'error', nodeId, message, hint })),
    ...save.edgeIds
      .filter((id) => edgeIds.has(id))
      .map((edgeId): FlowIssue => ({ code: 'save', severity: 'error', edgeId, message, hint })),
  ];
}

// Pure and I/O free. Issues never block a run. Errors come first, then
// warnings, each group in node order.
export function computeFlowIssues(
  nodes: FlowNode[],
  edges: FlowEdge[],
  ctx: FlowIssueContext = {},
): FlowIssue[] {
  const reaching = nodesReachingOutput(nodes, edges);
  const issues: FlowIssue[] = [];
  for (const node of nodes) {
    issues.push(
      ...expressionIssues(node),
      ...inputIssues(node, edges),
      ...requestIssues(node, edges),
      ...switchIssues(node),
      ...waitIssues(node, nodes),
      ...outputIssues(node, edges),
      ...exitIssues(node, edges),
    );
    if (reaching && node.kind.kind !== 'Output' && !reaching.has(node.id)) {
      issues.push(
        issue(
          'no-path-to-output',
          'warning',
          node,
          'This node does not lead to any Output, so its result is never shown.',
        ),
      );
    }
  }
  if (ctx.save) {
    issues.push(
      ...saveIssues(ctx.save, new Set(nodes.map((n) => n.id)), new Set(edges.map((e) => e.id))),
    );
  }
  const rank = (i: FlowIssue) => (i.severity === 'error' ? 0 : 1);
  return issues.sort((a, b) => rank(a) - rank(b));
}

export function groupIssuesByNode(issues: FlowIssue[]): Map<string, FlowIssue[]> {
  const grouped = new Map<string, FlowIssue[]>();
  for (const i of issues) {
    if (!i.nodeId) continue;
    grouped.set(i.nodeId, [...(grouped.get(i.nodeId) ?? []), i]);
  }
  return grouped;
}

export function worstSeverity(issues: FlowIssue[]): IssueSeverity | null {
  if (issues.some((i) => i.severity === 'error')) return 'error';
  return issues.length > 0 ? 'warning' : null;
}

const severityWord = (i: FlowIssue) => (i.severity === 'error' ? 'Error' : 'Warning');

// The accessible name of a node's badge.
export function summarizeIssues(issues: FlowIssue[]): string {
  const lines = issues.map((i) => `${severityWord(i)}: ${i.message}`);
  return issues.length === 1 ? lines[0] : `${issues.length} issues: ${lines.join(' ')}`;
}

// The accessible name of the count button, such as "2 errors, 1 warning".
export function issueCountLabel(issues: FlowIssue[]): string {
  const errors = issues.filter((i) => i.severity === 'error').length;
  const warnings = issues.length - errors;
  const parts = [
    errors > 0 ? `${errors} error${errors === 1 ? '' : 's'}` : null,
    warnings > 0 ? `${warnings} warning${warnings === 1 ? '' : 's'}` : null,
  ].filter((p): p is string => p !== null);
  return parts.length > 0 ? parts.join(', ') : 'No issues';
}
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `yarn test src/lib/__tests__/flow-issues.test.ts`
Expected: PASS. If the `cleanSaveMessage` cycle test fails, print the intermediate string: the order of the three `replace` calls matters (prefix, then ids, then the dangling "through").

- [ ] **Step 5: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-issues.ts src/lib/__tests__/flow-issues.test.ts`
Suggested subject: `feat(flow): compute client-side flow issues`.

---

### Task 2: Badge, ring and the eight node components

**Files:**
- Create: `src/components/flow/nodes/NodeIssueBadge.tsx`
- Create: `src/components/flow/nodes/__tests__/NodeIssueBadge.test.tsx`
- Modify: `src/components/flow/nodes/nodeStatus.ts`
- Modify: `src/components/flow/nodes/__tests__/nodeStatus.test.ts`
- Modify: `src/components/flow/nodes/AuthNode.tsx`, `IfNode.tsx`, `InputNode.tsx`, `OutputNode.tsx`, `RequestNode.tsx`, `SwitchNode.tsx`, `TransformNode.tsx`, `WaitForCallbackNode.tsx`
- Modify: `src/components/flow/nodes/__tests__/RequestNode.test.tsx`
- Modify: `src/components/flow/FlowCanvas.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (compute `issues`, pass them to the canvas)
- Create: `src/components/flow/__tests__/FlowCanvas.issues.test.tsx`

**Interfaces:**
- Consumes: `FlowIssue`, `groupIssuesByNode`, `worstSeverity`, `summarizeIssues`, `computeFlowIssues` from Task 1.
- Produces: `<NodeIssueBadge issues={FlowIssue[] | undefined} />` (renders nothing for none; icon only, `role='img'`, `aria-label` = `summarizeIssues`, `data-testid='node-issue-badge'`, `data-severity`).
- Produces: `issueRingClassName(issues?: FlowIssue[]): string | undefined` (`'ring-2 ring-red-500'` for an error, `'ring-1 ring-amber-500'` for a warning, otherwise `undefined`).
- Produces: each node's `data.issues?: FlowIssue[]` replacing `data.hasCycleError`. `FlowCanvasProps.issues?: FlowIssue[]` replaces `cycleNodeIds`.

- [ ] **Step 1: Write the failing tests**

1. Create `src/components/flow/nodes/__tests__/NodeIssueBadge.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import { NodeIssueBadge } from '../NodeIssueBadge';

const error: FlowIssue = { code: 'expr-blank', severity: 'error', nodeId: 'n1', message: 'Boom.' };
const warning: FlowIssue = { code: 'exit-unwired', severity: 'warning', nodeId: 'n1', message: 'Careful.' };

describe('NodeIssueBadge', () => {
  it('renders nothing without issues', () => {
    const { container, rerender } = render(<NodeIssueBadge issues={undefined} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<NodeIssueBadge issues={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('names a single error and marks its severity', () => {
    render(<NodeIssueBadge issues={[error]} />);
    const badge = screen.getByRole('img', { name: 'Error: Boom.' });
    expect(badge).toHaveAttribute('data-severity', 'error');
  });

  it('shows the worst severity and names every issue', () => {
    render(<NodeIssueBadge issues={[warning, error]} />);
    const badge = screen.getByTestId('node-issue-badge');
    expect(badge).toHaveAttribute('data-severity', 'error');
    expect(badge).toHaveAccessibleName('2 issues: Warning: Careful. Error: Boom.');
  });

  it('adds no text to the node card, only an icon', () => {
    render(<NodeIssueBadge issues={[error, warning]} />);
    expect(screen.getByTestId('node-issue-badge').textContent).toBe('');
  });
});
```

2. In `src/components/flow/nodes/__tests__/nodeStatus.test.ts`, change the import to `import { issueRingClassName, nodeStatusCaption, nodeStatusClassName } from '../nodeStatus';` and append:

```ts
describe('issueRingClassName', () => {
  const error = { code: 'a', severity: 'error' as const, nodeId: 'n', message: 'm' };
  const warning = { code: 'b', severity: 'warning' as const, nodeId: 'n', message: 'm' };

  it('draws a red ring for an error, even next to a warning', () => {
    expect(issueRingClassName([error])).toContain('ring-red-500');
    expect(issueRingClassName([warning, error])).toContain('ring-red-500');
  });

  it('draws an amber ring, and no red, for a warning', () => {
    const cls = issueRingClassName([warning]);
    expect(cls).toContain('ring-amber-500');
    expect(cls).not.toContain('ring-red-500');
  });

  it('draws no ring without issues', () => {
    expect(issueRingClassName(undefined)).toBeUndefined();
    expect(issueRingClassName([])).toBeUndefined();
  });
});
```

3. In `src/components/flow/nodes/__tests__/RequestNode.test.tsx`, replace the test `'outlines the card when it is part of a rejected cycle'` with:

```tsx
  it('outlines the card in red when the node has an error', () => {
    renderNode({
      kind: baseKind,
      status: 'idle',
      issues: [{ code: 'save', severity: 'error', nodeId: 'n1', message: 'Rejected.' }],
    });
    expect(screen.getByTestId('request-node-card').className).toContain('ring-red-500');
  });

  it('outlines the card in amber, not red, when the node only has a warning', () => {
    renderNode({
      kind: baseKind,
      status: 'idle',
      issues: [{ code: 'exit-unwired', severity: 'warning', nodeId: 'n1', message: 'Careful.' }],
    });
    const cls = screen.getByTestId('request-node-card').className;
    expect(cls).toContain('ring-amber-500');
    expect(cls).not.toContain('ring-red-500');
  });
```

4. Create `src/components/flow/__tests__/FlowCanvas.issues.test.tsx`, which renders all eight node kinds through the real canvas:

```tsx
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

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

const at = { x: 0, y: 0 };
const nodes: FlowNode[] = [
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

const issueFor = (nodeId: string, severity: FlowIssue['severity']): FlowIssue => ({
  code: 'test',
  severity,
  nodeId,
  message: severity === 'error' ? 'Boom.' : 'Careful.',
});

function renderCanvas(issues?: FlowIssue[]) {
  return render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      issues={issues}
    />,
  );
}

const card = (id: string) => document.querySelector<HTMLElement>(`[data-testid="${id}"]`);

describe('FlowCanvas issue badges', () => {
  it('draws no ring and no badge without issues', () => {
    renderCanvas();
    for (const id of cardIds) {
      const el = card(id);
      expect(el, id).not.toBeNull();
      expect(el?.className).not.toContain('ring-');
      expect(el?.querySelector('[data-testid="node-issue-badge"]')).toBeNull();
    }
  });

  it('gives all eight node kinds a red ring and an error badge', () => {
    renderCanvas(nodes.map((n) => issueFor(n.id, 'error')));
    for (const id of cardIds) {
      const el = card(id);
      expect(el?.className, id).toContain('ring-red-500');
      const badge = el?.querySelector('[data-testid="node-issue-badge"]');
      expect(badge, id).toHaveAttribute('data-severity', 'error');
      expect(badge, id).toHaveAttribute('aria-label', 'Error: Boom.');
    }
  });

  it('gives all eight node kinds an amber ring and a warning badge, never red', () => {
    renderCanvas(nodes.map((n) => issueFor(n.id, 'warning')));
    for (const id of cardIds) {
      const el = card(id);
      expect(el?.className, id).toContain('ring-amber-500');
      expect(el?.className, id).not.toContain('ring-red-500');
      expect(el?.querySelector('[data-testid="node-issue-badge"]'), id).toHaveAttribute(
        'data-severity',
        'warning',
      );
    }
  });

  it('marks only the nodes that have issues', () => {
    renderCanvas([issueFor('out', 'error')]);
    expect(card('output-node-card')?.className).toContain('ring-red-500');
    expect(card('input-node-card')?.className).not.toContain('ring-');
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/nodes/__tests__/NodeIssueBadge.test.tsx src/components/flow/nodes/__tests__/nodeStatus.test.ts src/components/flow/nodes/__tests__/RequestNode.test.tsx src/components/flow/__tests__/FlowCanvas.issues.test.tsx`
Expected: FAIL (cannot resolve `../NodeIssueBadge`; `issueRingClassName` is not exported; no `issues` prop).

- [ ] **Step 3: Add the ring helper**

In `src/components/flow/nodes/nodeStatus.ts`, add the imports at the top and the function at the end:

```ts
import { type FlowIssue, worstSeverity } from '@/lib/flow-issues';
```

```ts
// Ring around a node card for the worst problem it has. Plain colours only.
export function issueRingClassName(issues?: FlowIssue[]): string | undefined {
  const worst = worstSeverity(issues ?? []);
  if (worst === 'error') return 'ring-2 ring-red-500';
  if (worst === 'warning') return 'ring-1 ring-amber-500';
  return undefined;
}
```

- [ ] **Step 4: Create the badge**

Create `src/components/flow/nodes/NodeIssueBadge.tsx`:

```tsx
import { CircleAlert, TriangleAlert } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { type FlowIssue, summarizeIssues, worstSeverity } from '@/lib/flow-issues';
import { cn } from '@/lib/utils';

// A small icon in a node header. It carries no text, so the node card's text
// stays the same. The accessible name lists every issue, and the tooltip shows
// the same lines on hover.
export function NodeIssueBadge({ issues }: { issues?: FlowIssue[] }) {
  if (!issues || issues.length === 0) return null;
  const severity = worstSeverity(issues);
  const Icon = severity === 'error' ? CircleAlert : TriangleAlert;
  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            role='img'
            aria-label={summarizeIssues(issues)}
            data-testid='node-issue-badge'
            data-severity={severity}
            className={cn(
              'inline-flex shrink-0',
              severity === 'error' ? 'text-red-500' : 'text-amber-500',
            )}
          >
            <Icon className='h-3.5 w-3.5' aria-hidden='true' />
          </span>
        </TooltipTrigger>
        <TooltipContent className='max-w-64 space-y-1'>
          {issues.map((i) => (
            <p key={`${i.code}:${i.message}`}>{i.message}</p>
          ))}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
```

- [ ] **Step 5: Update the eight node components**

In each of `AuthNode.tsx`, `IfNode.tsx`, `InputNode.tsx`, `OutputNode.tsx`, `RequestNode.tsx`, `SwitchNode.tsx`, `TransformNode.tsx` and `WaitForCallbackNode.tsx` (all under `src/components/flow/nodes/`) make the same four edits:

1. In the node's data interface or type, replace the `hasCycleError?: boolean;` line and the doc comment above it with:

```ts
  /** Problems found in this node, drawn as a ring and a badge. */
  issues?: FlowIssue[];
```

2. Add imports (keep Biome's order):

```ts
import type { FlowIssue } from '@/lib/flow-issues';
import { NodeIssueBadge } from './NodeIssueBadge';
```

and add `issueRingClassName` to the existing `./nodeStatus` import, so it reads `import { issueRingClassName, nodeStatusClassName } from './nodeStatus';`.

3. In the card's `cn(...)` call, replace the line `data.hasCycleError && 'ring-2 ring-red-500',` with:

```ts
        issueRingClassName(data.issues),
```

4. In the card header, add this line immediately above `<NodeMenuButton`:

```tsx
        <NodeIssueBadge issues={data.issues} />
```

(`RequestNode` has the `Bug` icon block above its `NodeMenuButton`; put the badge between the two.) Nothing else in the nodes changes.

- [ ] **Step 6: Update `FlowCanvas.tsx`**

1. Add the import:

```tsx
import { type FlowIssue, groupIssuesByNode } from '@/lib/flow-issues';
```

2. In `FlowCanvasProps`, replace the `cycleNodeIds` prop and its comment with:

```tsx
  // Problems to show on nodes. Computed by the owner so the same list feeds the issue count.
  issues?: FlowIssue[];
```

(Keep `cycleEdgeIds` as it is: wires still turn red through `toRfEdges`.)

3. Above `toRfNodes`, add:

```tsx
const NO_ISSUES: ReadonlyMap<string, FlowIssue[]> = new Map();
```

4. In `toRfNodes`, replace the parameter `cycleNodeIds?: string[],` with `issuesByNode: ReadonlyMap<string, FlowIssue[]> = NO_ISSUES,` (it sits before the defaulted `savedPreviews` parameter), and replace the data line `hasCycleError: cycleNodeIds?.includes(n.id) ?? false,` with:

```tsx
        issues: issuesByNode.get(n.id),
```

5. In `FlowCanvasInner`, replace `cycleNodeIds,` in the destructured props with `issues,`. Before `rfNodes`, add:

```tsx
  const issuesByNode = useMemo(() => groupIssuesByNode(issues ?? []), [issues]);
```

and in the `toRfNodes(...)` call replace the argument `cycleNodeIds,` with `issuesByNode,`; in that `useMemo` dependency array replace `cycleNodeIds` with `issuesByNode`.

- [ ] **Step 7: Compute and pass `issues` in `FlowPane.tsx`**

1. Change the React import to `import { useCallback, useEffect, useMemo, useRef, useState } from 'react';` and add `import { computeFlowIssues } from '@/lib/flow-issues';` next to the other `@/lib/` imports.

2. After the `useClearRemovedAuthTokens(...)` line (it is above the picker early return, so the hook order stays stable), add:

```tsx
  // Client-side checks plus whatever the last rejected save named. The popover
  // count and the node badges read this one list.
  const issues = useMemo(
    () =>
      computeFlowIssues(tab.nodes, tab.edges, {
        save:
          cycleNodeIds.length > 0 || cycleEdgeIds.length > 0
            ? { nodeIds: cycleNodeIds, edgeIds: cycleEdgeIds, message: saveErrorMessage }
            : undefined,
      }),
    [tab.nodes, tab.edges, cycleNodeIds, cycleEdgeIds, saveErrorMessage],
  );
```

3. On `<FlowCanvas`, replace `cycleNodeIds={cycleNodeIds}` with `issues={issues}`. Leave `cycleEdgeIds={cycleEdgeIds}` and the panel's `saveError` prop alone.

- [ ] **Step 8: Run to verify the tests pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-issues.test.ts`
Expected: PASS. In particular the three red-ring tests in `FlowPane.test.tsx` must pass unchanged: they flag nodes `a` and `b` (red) while `c` gets only an amber ring, and the card text is unchanged because the badge has no text. If one of them fails on `textContent`, the badge is leaking text: check that no `<span>` text or tooltip content is rendered outside the portal.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors. A leftover `hasCycleError` anywhere is a type error; `grep -rn hasCycleError src` must return nothing.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/nodes/NodeIssueBadge.tsx src/components/flow/nodes/nodeStatus.ts src/components/flow/nodes/AuthNode.tsx src/components/flow/nodes/IfNode.tsx src/components/flow/nodes/InputNode.tsx src/components/flow/nodes/OutputNode.tsx src/components/flow/nodes/RequestNode.tsx src/components/flow/nodes/SwitchNode.tsx src/components/flow/nodes/TransformNode.tsx src/components/flow/nodes/WaitForCallbackNode.tsx src/components/flow/nodes/__tests__/NodeIssueBadge.test.tsx src/components/flow/nodes/__tests__/nodeStatus.test.ts src/components/flow/nodes/__tests__/RequestNode.test.tsx src/components/flow/FlowCanvas.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowCanvas.issues.test.tsx`
Suggested subject: `feat(flow): show issue badges and severity rings on nodes`.

---

### Task 3: Issue count and list next to Run

**Files:**
- Create: `src/components/flow/FlowIssuesButton.tsx`
- Create: `src/components/flow/__tests__/FlowIssuesButton.test.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (selection handler, button in the top-right container)
- Create: `src/components/flow/__tests__/FlowPane.issues.test.tsx`

**Interfaces:**
- Consumes: `FlowIssue`, `issueCountLabel`, `worstSeverity` from Task 1; `issues` from `FlowPane` (Task 2).
- Produces: `<FlowIssuesButton issues={FlowIssue[]} nodes={FlowNode[]} onSelectNode={(nodeId: string) => void} />`. Renders nothing for an empty list. The trigger's accessible name is `issueCountLabel(issues)`. The popover list is a `<ul aria-label='Flow issues'>`.

- [ ] **Step 1: Write the failing tests**

1. Create `src/components/flow/__tests__/FlowIssuesButton.test.tsx`:

```tsx
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowIssuesButton } from '../FlowIssuesButton';

// Radix popovers call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

const nodes: FlowNode[] = [
  { id: 'n1', position: { x: 0, y: 0 }, kind: { kind: 'If', label: 'Check', condition: '' } },
  { id: 'n2', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
];

const issues: FlowIssue[] = [
  { code: 'expr-blank', severity: 'error', nodeId: 'n1', message: 'The If node condition is empty.' },
  { code: 'output-no-value', severity: 'warning', nodeId: 'n2', message: 'No value is wired.' },
  { code: 'save', severity: 'error', edgeId: 'e1', message: 'Bad wire.' },
];

describe('FlowIssuesButton', () => {
  it('renders nothing without issues', () => {
    const { container } = render(<FlowIssuesButton issues={[]} nodes={nodes} onSelectNode={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('names the count by severity', () => {
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={vi.fn()} />);
    expect(screen.getByRole('button', { name: '2 errors, 1 warning' })).toHaveTextContent('3');
  });

  it('lists every issue, with the node label, in the popover', async () => {
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: '2 errors, 1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(issues.length);
    expect(list).toHaveTextContent('Check');
    expect(list).toHaveTextContent('The If node condition is empty.');
  });

  it('selects the node when its item is clicked, and closes the list', async () => {
    const onSelectNode = vi.fn();
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={onSelectNode} />);
    await userEvent.click(screen.getByRole('button', { name: '2 errors, 1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    await userEvent.click(within(list).getAllByRole('button')[0]);
    expect(onSelectNode).toHaveBeenCalledWith('n1');
    expect(screen.queryByRole('list', { name: 'Flow issues' })).not.toBeInTheDocument();
  });

  it('does not make an issue without a node clickable', async () => {
    render(<FlowIssuesButton issues={issues} nodes={nodes} onSelectNode={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: '2 errors, 1 warning' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    // Two issues name a node, so two buttons. The edge issue is plain text.
    expect(within(list).getAllByRole('button')).toHaveLength(2);
    expect(list).toHaveTextContent('Bad wire.');
  });
});
```

2. Create `src/components/flow/__tests__/FlowPane.issues.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), listFlows: vi.fn(), saveFlow: vi.fn() };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));
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

// Radix menus and popovers call APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

// jsdom reports every rect as 0,0,0,0 and userEvent clicks at 0,0, so the resize
// handle would count as hit by every click. Park the handle away from the pointer.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const tabId = 'flow-issues-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: issues',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'issues',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'a' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'if1', position: { x: 0, y: 200 }, kind: { kind: 'If', label: 'Check', condition: 'true' } },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
  ],
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

describe('FlowPane issues', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('counts the problems of the flow next to Run and badges the node', () => {
    render(<Harness />);
    // The If node has no input (error), both its exits are unwired and it leads to
    // no Output (two warnings).
    expect(screen.getByRole('button', { name: '1 error, 2 warnings' })).toBeInTheDocument();
    const card = screen.getByTestId('if-node-card');
    expect(card.className).toContain('ring-red-500');
    expect(within(card).getByTestId('node-issue-badge')).toHaveAttribute('data-severity', 'error');
  });

  it('opens the node panel when an issue is chosen from the list', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: '1 error, 2 warnings' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    await userEvent.click(within(list).getAllByRole('button')[0]);
    expect(await screen.findByTestId('node-properties-panel')).toHaveTextContent('Check');
    expect(screen.getByRole('tab', { name: 'Settings' })).toHaveAttribute('aria-selected', 'true');
  });

  it('does not block a run or a save because of an issue', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
  });

  it('adds a rejected save to the count and keeps the named node red', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow is invalid: the Output has a problem — node(s): out1; edge(s): ',
    );
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await screen.findByRole('button', { name: '2 errors, 2 warnings' });
    expect(screen.getByTestId('output-node-card').className).toContain('ring-red-500');
    expect(screen.getByTestId('input-node-card').className).not.toContain('ring-red-500');
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowIssuesButton.test.tsx src/components/flow/__tests__/FlowPane.issues.test.tsx`
Expected: FAIL (cannot resolve `../FlowIssuesButton`).

- [ ] **Step 3: Create the button**

Create `src/components/flow/FlowIssuesButton.tsx`:

```tsx
import { CircleAlert, TriangleAlert } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { type FlowIssue, issueCountLabel, worstSeverity } from '@/lib/flow-issues';
import type { FlowNode } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface FlowIssuesButtonProps {
  issues: FlowIssue[];
  nodes: FlowNode[];
  onSelectNode: (nodeId: string) => void;
}

function SeverityIcon({ severity }: { severity: FlowIssue['severity'] }) {
  const Icon = severity === 'error' ? CircleAlert : TriangleAlert;
  return (
    <Icon
      className={cn(
        'mt-0.5 h-3.5 w-3.5 shrink-0',
        severity === 'error' ? 'text-red-500' : 'text-amber-500',
      )}
      aria-hidden='true'
    />
  );
}

// A count next to Run. The popover lists every issue, and a click on one that
// names a node selects it and opens its panel.
export function FlowIssuesButton({ issues, nodes, onSelectNode }: FlowIssuesButtonProps) {
  const [open, setOpen] = useState(false);
  if (issues.length === 0) return null;
  const worst = worstSeverity(issues) ?? 'warning';
  const nodeLabel = (nodeId: string) => {
    const node = nodes.find((n) => n.id === nodeId);
    return node ? node.kind.label.trim() || node.id : nodeId;
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='gap-1.5'
          aria-label={issueCountLabel(issues)}
        >
          <SeverityIcon severity={worst} />
          <span>{issues.length}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent align='end' className='nokey w-80 p-1'>
        <ul aria-label='Flow issues' className='max-h-80 overflow-y-auto'>
          {issues.map((issue) => {
            const nodeId = issue.nodeId;
            const key = `${issue.code}:${nodeId ?? issue.edgeId ?? ''}:${issue.message}`;
            return (
              <li key={key}>
                {nodeId ? (
                  <Button
                    type='button'
                    variant='ghost'
                    size='sm'
                    className='h-auto w-full items-start justify-start gap-2 whitespace-normal px-2 py-1.5 text-left text-xs'
                    onClick={() => {
                      onSelectNode(nodeId);
                      setOpen(false);
                    }}
                  >
                    <SeverityIcon severity={issue.severity} />
                    <span>
                      <span className='font-medium'>{nodeLabel(nodeId)}</span>
                      {': '}
                      {issue.message}
                    </span>
                  </Button>
                ) : (
                  <div className='flex items-start gap-2 px-2 py-1.5 text-xs'>
                    <SeverityIcon severity={issue.severity} />
                    <span>
                      <span className='font-medium'>Wire</span>
                      {': '}
                      {issue.message}
                    </span>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 4: Wire it into `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

1. Add the import: `import { FlowIssuesButton } from './FlowIssuesButton';`.

2. After the `handleOpenProperties` definition (above the early return), add:

```tsx
  // Selects the node an issue names and opens its Settings tab.
  const handleSelectIssueNode = useCallback(
    (nodeId: string) => {
      setPanelTab('settings');
      handleSelectedNodeIdsChange(new Set([nodeId]));
      handleOpenProperties(nodeId);
    },
    [handleOpenProperties, handleSelectedNodeIdsChange],
  );
```

3. In the top-right container (`absolute top-2 right-2 z-10 flex items-center gap-2`), directly before `<FlowToolbar`, add:

```tsx
            <FlowIssuesButton
              issues={issues}
              nodes={tab.nodes}
              onSelectNode={handleSelectIssueNode}
            />
```

- [ ] **Step 5: Run to verify the tests pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-issues.test.ts`
Expected: PASS. If `getByRole('button', { name: '1 error, 2 warnings' })` finds nothing in `FlowPane.issues.test.tsx`, print `computeFlowIssues(baseTab.nodes, baseTab.edges)` once: the expected list is `input-missing` (error) for `if1`, plus `exit-unwired` and `no-path-to-output` (warnings) for `if1`, and nothing for `in1` and `out1`.

- [ ] **Step 6: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/FlowIssuesButton.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowIssuesButton.test.tsx src/components/flow/__tests__/FlowPane.issues.test.tsx`
Suggested subject: `feat(flow): list flow issues next to Run`.

---

## Self-Review

- **Spec coverage (F-37a):** `FlowIssue` shape, `computeFlowIssues` and every client rule from the notes: blank If/Switch/Transform expression (V8), no input (V1), Output without a value wire, empty saved path or inline URL (with the wired-url exception), duplicate Switch matches (V7), empty or duplicate Wait name and bad timeout (V10), repeat-until limits (V9), warnings for unwired exits (If exit, Switch case and default merged into one warning per node), and a node with no path to an Output (Task 1). Save-error ids folded in as `code: 'save'` errors (Tasks 1 and 2). `hasCycleError` replaced by `issues` in `toRfNodes`, shared `NodeIssueBadge` with `CircleAlert` and `TriangleAlert`, tooltip, severity ring on all eight node files (Task 2). Count next to Run, popover list, click selects the node and opens its panel (Task 3). `useMemo` keyed on nodes, edges and save state (Task 2). Issues never block a run (Task 3 test).
- **Placeholders:** none. Every code step shows code. The eight node edits are one repeated four-step recipe with the exact lines to change, because the files differ only in their header markup.
- **Type consistency:** `FlowIssue` is imported from `@/lib/flow-issues` in the badge, `nodeStatus.ts`, the eight nodes, `FlowCanvas` and the button. `issues` is the prop name on `FlowCanvas`, the node `data` key and the `FlowPane` variable. `data-testid='node-issue-badge'` and `data-severity` match between the component and all three test files. The list label `Flow issues` and the trigger names from `issueCountLabel` match between the component and the tests.
- **Review Focus coverage:** item 1 is the save folding and filtering tests (Task 1), the "keeps the named node red" test (Task 3) and the unchanged existing `FlowPane.test.tsx` red-ring tests (Task 2 Step 8); item 2 is the `adds no text` badge test; item 3 is the amber-not-red canvas test over all eight kinds and `issueRingClassName`; item 4 is the healthy-flow, wired-url, no-Output and Auth-chain tests; item 5 is the `FlowIssuesButton` tests and the panel-opens `FlowPane` test.
- **Known cost:** `issues` is recomputed on every `tab.nodes` change, including each drag move, because positions live in `nodes`. The work is linear in nodes plus edges and `toRfNodes` already rebuilds every node object on that change, so this adds no extra React Flow re-render. If profiling shows a problem, key the memo on a position-free signature of the graph.
