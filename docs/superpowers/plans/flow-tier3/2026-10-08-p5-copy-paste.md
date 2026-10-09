# Flow Copy, Paste and Duplicate Implementation Plan

> **Execute this plan:** P5. Before starting it, make sure these are merged to main: P4 (must be merged first; both edit the canvas files). After it is merged, the next plan to execute is P6. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Copy, paste and duplicate flow nodes with Ctrl+C, Ctrl+V and Ctrl+D (and a Duplicate entry in the Request node menu), keeping the wires between the copied nodes and applying the per-kind rules for Auth, Switch and Wait for callback nodes.

**Architecture:** An in-memory clipboard (module variable, never the system clipboard) lives in `src/lib/flow-clipboard.ts` together with the pure functions `copySelection` and `instantiatePaste`. `FlowCanvas` turns the keys into `onCopy`, `onPaste`, `onDuplicate` callbacks. `FlowPane` owns the logic: it reads the latest graph from the store, builds the pasted nodes and wires, and adds them with one `updateFlowGraph` call, which is one undo step (P4). Frontend only.

**Tech Stack:** React, TypeScript, Zustand (`pane-store`), `@xyflow/react` 12, Vitest and Testing Library, shadcn `DropdownMenu`, `sonner` toasts.

**Spec:** Roadmap item F-35 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P5) and `00-plan-index.md` (plan P5).

**Depends on P4 (merged first).** Used verbatim from P4: `updateFlowGraph(tabId, nodes, edges)` called without options is exactly one undo step; `undoFlow(tabId)`; `FlowTab.history`; the `handleKeyDown` shape in `FlowCanvas.tsx` (a `const key = e.key.toLowerCase()` followed by one `if (key === ...) { ...; return; }` branch per key); the `CANVAS_HINTS` array in `FlowCanvas.tsx`; `pruneSelection`. P5 adds no store API.

## Global Constraints

- Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. This plan creates flow nodes that hold auth configuration and request data, and it must keep the flow file shape valid.
- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No `unwrap()` and no Rust changes in this plan.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <pattern>` listed in the task.
- Never read `flow-auth-store` while copying or pasting. A pasted Auth node starts without an in-memory token. Never log or toast credential values.
- Persistence stays unchanged: pasted nodes are ordinary `FlowNode` and `FlowEdge` values.
- Coordination with other plans: P1 mounts `FlowSaveShortcut` and adds a `tabId` prop to `FlowToolbar`; P2 adds a result strip; P4 adds undo handlers, selectors and `FlowHistoryButtons` to `FlowPane.tsx`. This plan adds its code after the P4 `handleGestureEnd` callback and its prop lines after the P4 `onRedo={handleRedo}` prop. It does not touch `FlowToolbar.tsx`, `handleSave`, `handleBeforeRun` or the result strip.
- If `yarn check` reports only formatting problems in files this plan touched, fix them with `yarn biome format --write <those paths>` (not the repo-wide `yarn format`).
- Not in scope: the system clipboard, cut (Ctrl+X), copying edges alone, copying between different collections when saved requests are involved (refused, see Task 2), rewriting `{{callback.<name>}}` text inside pasted requests, a Duplicate entry in the menu of node kinds that have no menu today.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A pasted Auth node must not apply to inherited auth, or the saved flow fails V13 ("only one Auth node can apply to inherited auth"). Test pinned in Task 1 (`instantiatePaste`) and Task 2 (store result).
2. A pasted Switch must have new case ids, and its wires must follow them. If the wires kept `case:<old id>` the pasted node's wires would leave through handles that do not exist. Test pinned in Task 1.
3. A pasted Wait for callback must get a unique `name` (V10), including against names already used by earlier pastes. Test pinned in Task 1 and Task 2.
4. One paste must be one undo step, and repeated pastes must not stack on the same spot. Tests pinned in Task 2 (single undo, growing offset) and Task 3 (keys on the real canvas).
5. Wires must only be copied when both ends are copied, and the clip must be a deep copy: editing a pasted node must never change the original. Tests pinned in Task 1.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/flow-ids.ts` (new) | `newNodeId(prefix)` (moved out of `NodePalette.tsx`) and `newEntityId()` for ids that need no readable prefix. |
| `src/lib/__tests__/flow-ids.test.ts` (new) | Id tests. |
| `src/lib/flow-clipboard.ts` (new) | `copySelection`, `instantiatePaste`, `uniqueCallbackName`, `canPasteInto`, and the in-memory clipboard. |
| `src/lib/__tests__/flow-clipboard.test.ts` (new) | Per-kind rule tests. |
| `src/components/flow/NodePalette.tsx` (modify) | Imports `newNodeId` from `flow-ids`. |
| `src/components/flow/FlowCanvas.tsx` (modify) | `onCopy`, `onPaste`, `onDuplicate`, `onDuplicateNode` props, three key branches, hint entry, `duplicateNode` action. |
| `src/components/flow/nodes/FlowNodeActionsContext.tsx` (modify) | Optional `duplicateNode` action. |
| `src/components/flow/nodes/NodeMenuButton.tsx` (modify) | "Duplicate" menu item when the action exists. |
| `src/components/flow/FlowPane.tsx` (modify) | Copy, paste and duplicate handlers; props for `FlowCanvas`. |
| `src/components/flow/__tests__/FlowCanvas.clipboard.test.tsx` (new) | Key handling. |
| `src/components/flow/__tests__/FlowPane.clipboard.test.tsx` (new) | Handler logic with a stand-in canvas. |
| `src/components/flow/__tests__/FlowPane.clipboardKeys.test.tsx` (new) | End to end with the real canvas, and the node menu entry. |

Existing tests to know: `src/components/flow/__tests__/FlowPane.multiselect.test.tsx` (stand-in canvas pattern), `src/components/flow/__tests__/FlowPane.delete.test.tsx` (real canvas inside `FlowPane`, needs the `DOMMatrixReadOnly` stub and the resize handle rect patch), `src/components/flow/__tests__/FlowCanvas.test.tsx` (`SelectHarness`), `src/components/flow/nodes/__tests__/RequestNode.test.tsx` (builds its own `FlowNodeActions` object, which is why the new action is optional).

---

### Task 1: Id helpers and the clipboard functions

**Files:**
- Create: `src/lib/flow-ids.ts`
- Create: `src/lib/__tests__/flow-ids.test.ts`
- Create: `src/lib/flow-clipboard.ts`
- Create: `src/lib/__tests__/flow-clipboard.test.ts`
- Modify: `src/components/flow/NodePalette.tsx` (lines 27-31: the local `nextId` and `newNodeId`)

**Interfaces:**
- Produces:

```ts
// src/lib/flow-ids.ts
export function newNodeId(prefix: string): string;
export function newEntityId(): string;

// src/lib/flow-clipboard.ts
export const PASTE_OFFSET = 40;
export interface FlowClip { collection: string | null; nodes: FlowNode[]; edges: FlowEdge[] }
export interface PasteResult { nodes: FlowNode[]; edges: FlowEdge[]; notices: string[] }
export function copySelection(nodes: FlowNode[], edges: FlowEdge[], ids: ReadonlySet<string>, collection: string | null): FlowClip | null;
export function instantiatePaste(clip: FlowClip, existingNodes: FlowNode[], step?: number): PasteResult;
export function uniqueCallbackName(name: string, taken: ReadonlySet<string>): string;
export function canPasteInto(clip: FlowClip, collection: string | null): string | null;
export function setFlowClipboard(clip: FlowClip): void;
export function getFlowClipboard(): FlowClip | null;
export function nextPasteStep(): number;
export function clearFlowClipboard(): void;
```

- [ ] **Step 1: Write the failing id tests**

Create `src/lib/__tests__/flow-ids.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { newEntityId, newNodeId } from '../flow-ids';

describe('flow ids', () => {
  it('prefixes node ids and never repeats one', () => {
    const ids = new Set(Array.from({ length: 50 }, () => newNodeId('input')));
    expect(ids.size).toBe(50);
    for (const id of ids) expect(id.startsWith('input-')).toBe(true);
  });

  it('makes unique entity ids', () => {
    expect(newEntityId()).not.toBe(newEntityId());
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-ids.test.ts`
Expected: FAIL, cannot resolve `../flow-ids`.

- [ ] **Step 3: Write the id helpers and use them in the palette**

Create `src/lib/flow-ids.ts`:

```ts
let counter = 0;

// A readable node id such as `input-1760000000000-3`. The counter keeps ids unique within a millisecond.
export function newNodeId(prefix: string): string {
  counter += 1;
  return `${prefix}-${Date.now()}-${counter}`;
}

// A random id for things nobody reads: edges, Switch cases and pasted nodes.
export function newEntityId(): string {
  return crypto.randomUUID();
}
```

In `src/components/flow/NodePalette.tsx` delete these lines:

```tsx
let nextId = 0;
function newNodeId(prefix: string) {
  nextId += 1;
  return `${prefix}-${Date.now()}-${nextId}`;
}
```

and add this import next to the other `@/lib` imports (Biome keeps imports sorted; `flow-ids` sorts after `flow-callback` and before `flow-repeat`):

```tsx
import { newNodeId } from '@/lib/flow-ids';
```

- [ ] **Step 4: Run it to verify it passes**

Run: `yarn test src/lib/__tests__/flow-ids.test.ts src/components/flow`
Expected: PASS. The palette tests must stay green because the id format is unchanged.

- [ ] **Step 5: Write the failing clipboard tests**

Create `src/lib/__tests__/flow-clipboard.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_AUTH_NODE_AUTH } from '@/lib/flow-auth';
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import {
  canPasteInto,
  clearFlowClipboard,
  copySelection,
  getFlowClipboard,
  instantiatePaste,
  nextPasteStep,
  PASTE_OFFSET,
  setFlowClipboard,
  uniqueCallbackName,
} from '../flow-clipboard';

const at = (id: string, kind: FlowNode['kind'], x = 0, y = 0): FlowNode => ({
  id,
  kind,
  position: { x, y },
});

const wire = (id: string, from: string, to: string, over: Partial<FlowEdge> = {}): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'value',
  expression: 'response.body',
  ...over,
});

const input = (id: string, x = 0, y = 0) =>
  at(id, { kind: 'Input', label: id, value: 'v' }, x, y);
const output = (id: string, x = 0, y = 0) => at(id, { kind: 'Output', label: id }, x, y);

describe('copySelection', () => {
  it('returns null for an empty selection', () => {
    expect(copySelection([input('a')], [], new Set(), 'demo')).toBeNull();
    expect(copySelection([input('a')], [], new Set(['missing']), 'demo')).toBeNull();
  });

  it('keeps only wires whose two ends are both selected', () => {
    const nodes = [input('a'), output('b'), output('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c')];
    const clip = copySelection(nodes, edges, new Set(['a', 'b']), 'demo');
    expect(clip?.nodes.map((n) => n.id)).toEqual(['a', 'b']);
    expect(clip?.edges.map((e) => e.id)).toEqual(['e1']);
    expect(clip?.collection).toBe('demo');
  });

  it('drops the auth wire when the Auth node is not copied', () => {
    const auth = at('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: DEFAULT_AUTH_NODE_AUTH,
      applyToInherit: false,
    });
    const req = at('req', {
      kind: 'Request',
      label: 'R',
      source: { type: 'Saved', requestPath: 'a.yml' },
    });
    const edges = [wire('e1', 'auth', 'req', { targetField: 'auth' })];
    const clip = copySelection([auth, req], edges, new Set(['req']), 'demo');
    expect(clip?.edges).toEqual([]);
  });

  it('makes a deep copy, so later edits to the graph do not reach the clip', () => {
    const nodes = [input('a')];
    const clip = copySelection(nodes, [], new Set(['a']), 'demo');
    nodes[0].position.x = 999;
    expect(clip?.nodes[0].position.x).toBe(0);
  });
});

describe('instantiatePaste', () => {
  it('gives every node and wire a fresh id and remaps the wire ends', () => {
    const clip = copySelection(
      [input('a', 10, 20), output('b', 110, 20)],
      [wire('e1', 'a', 'b', { expression: 'response.body.id' })],
      new Set(['a', 'b']),
      'demo',
    );
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, [input('a'), output('b')]);
    expect(result.nodes).toHaveLength(2);
    const ids = result.nodes.map((n) => n.id);
    expect(new Set(ids).size).toBe(2);
    expect(ids).not.toContain('a');
    expect(ids).not.toContain('b');
    expect(result.edges).toHaveLength(1);
    const [edge] = result.edges;
    expect(edge.id).not.toBe('e1');
    expect(edge.sourceNodeId).toBe(ids[0]);
    expect(edge.targetNodeId).toBe(ids[1]);
    expect(edge.targetField).toBe('value');
    expect(edge.expression).toBe('response.body.id');
  });

  it('offsets positions by PASTE_OFFSET times the step', () => {
    const clip = copySelection([input('a', 10, 20)], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    expect(instantiatePaste(clip, []).nodes[0].position).toEqual({
      x: 10 + PASTE_OFFSET,
      y: 20 + PASTE_OFFSET,
    });
    expect(instantiatePaste(clip, [], 3).nodes[0].position).toEqual({
      x: 10 + 3 * PASTE_OFFSET,
      y: 20 + 3 * PASTE_OFFSET,
    });
  });

  it('does not touch the clip or the existing nodes', () => {
    const clip = copySelection([input('a')], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const snapshot = JSON.stringify(clip);
    const existing = [input('a')];
    instantiatePaste(clip, existing);
    expect(JSON.stringify(clip)).toBe(snapshot);
    expect(existing[0].id).toBe('a');
  });

  it('pastes an Auth node with applyToInherit off and says so', () => {
    const auth = at('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: { authType: 'bearer', token: '{{token}}' },
      applyToInherit: true,
    });
    const clip = copySelection([auth], [], new Set(['auth']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, [auth]);
    const pasted = result.nodes[0].kind;
    if (pasted.kind !== 'Auth') throw new Error('Expected an Auth node');
    expect(pasted.applyToInherit).toBe(false);
    expect(pasted.auth).toEqual({ authType: 'bearer', token: '{{token}}' });
    expect(result.notices).toHaveLength(1);
    expect(result.notices[0]).toMatch(/inherited auth/i);
    // The original keeps its setting and the clip is unchanged.
    expect(auth.kind.kind === 'Auth' && auth.kind.applyToInherit).toBe(true);
    expect(clip.nodes[0].kind.kind === 'Auth' && clip.nodes[0].kind.applyToInherit).toBe(true);
  });

  it('gives a pasted Auth node its own copy of the auth config', () => {
    const auth = at('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: { authType: 'basic', username: 'u', password: 'p' },
      applyToInherit: false,
    });
    const clip = copySelection([auth], [], new Set(['auth']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, [auth]);
    expect(result.notices).toEqual([]);
    const pasted = result.nodes[0].kind;
    if (pasted.kind !== 'Auth' || auth.kind.kind !== 'Auth') throw new Error('Expected Auth nodes');
    expect(pasted.auth).not.toBe(auth.kind.auth);
    expect(pasted.auth).toEqual(auth.kind.auth);
  });

  it('regenerates Switch case ids and follows them on the wires', () => {
    const sw = at('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [
        { id: 'c1', label: 'One', matches: '1' },
        { id: 'c2', label: 'Two', matches: '2' },
      ],
    });
    const edges = [
      wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('c1'), targetField: 'trigger' }),
      wire('e2', 'sw', 'o2', { sourceHandle: caseHandle('c2'), targetField: 'trigger' }),
      wire('e3', 'sw', 'o3', { sourceHandle: 'default', targetField: 'trigger' }),
    ];
    const nodes = [sw, output('o1'), output('o2'), output('o3')];
    const clip = copySelection(nodes, edges, new Set(['sw', 'o1', 'o2', 'o3']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, nodes);
    const pasted = result.nodes[0].kind;
    if (pasted.kind !== 'Switch') throw new Error('Expected a Switch node');
    const newIds = pasted.cases.map((c) => c.id);
    expect(newIds).toHaveLength(2);
    expect(newIds).not.toContain('c1');
    expect(newIds).not.toContain('c2');
    expect(pasted.cases.map((c) => c.label)).toEqual(['One', 'Two']);
    const handles = result.edges.map((e) => e.sourceHandle);
    expect(handles).toEqual([caseHandle(newIds[0]), caseHandle(newIds[1]), 'default']);
  });

  it('drops a wire that names a Switch case that is not on the node', () => {
    const sw = at('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [{ id: 'c1', label: 'One', matches: '1' }],
    });
    const nodes = [sw, output('o1')];
    const edges = [
      wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('gone'), targetField: 'trigger' }),
    ];
    const clip = copySelection(nodes, edges, new Set(['sw', 'o1']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    expect(instantiatePaste(clip, nodes).edges).toEqual([]);
  });

  it('keeps the source handle of an If wire', () => {
    const iff = at('if', { kind: 'If', label: 'If', condition: 'true' });
    const nodes = [iff, output('o1')];
    const edges = [wire('e1', 'if', 'o1', { sourceHandle: 'true', targetField: 'trigger' })];
    const clip = copySelection(nodes, edges, new Set(['if', 'o1']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    expect(instantiatePaste(clip, nodes).edges[0].sourceHandle).toBe('true');
  });

  it('renames a pasted Wait for callback to a free name and warns about the old variable', () => {
    const wait = at('w', {
      kind: 'WaitForCallback',
      label: 'Wait',
      name: 'pay',
      timeoutMs: 60000,
    });
    const clip = copySelection([wait], [], new Set(['w']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const first = instantiatePaste(clip, [wait]);
    const firstKind = first.nodes[0].kind;
    if (firstKind.kind !== 'WaitForCallback') throw new Error('Expected a Wait node');
    expect(firstKind.name).toBe('pay_2');
    expect(first.notices[0]).toContain('{{callback.pay}}');
    expect(first.notices[0]).toContain('pay_2');
    // A second paste sees the first one and moves on.
    const second = instantiatePaste(clip, [wait, ...first.nodes]);
    const secondKind = second.nodes[0].kind;
    if (secondKind.kind !== 'WaitForCallback') throw new Error('Expected a Wait node');
    expect(secondKind.name).toBe('pay_3');
    expect(secondKind.timeoutMs).toBe(60000);
  });

  it('keeps a Wait name that is free', () => {
    const wait = at('w', {
      kind: 'WaitForCallback',
      label: 'Wait',
      name: 'pay',
      timeoutMs: 60000,
    });
    const clip = copySelection([wait], [], new Set(['w']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, []);
    const kind = result.nodes[0].kind;
    if (kind.kind !== 'WaitForCallback') throw new Error('Expected a Wait node');
    expect(kind.name).toBe('pay');
    expect(result.notices).toEqual([]);
  });

  it('never gives two pasted Wait nodes the same name', () => {
    const w1 = at('w1', { kind: 'WaitForCallback', label: 'A', name: 'a', timeoutMs: 1000 });
    const w2 = at('w2', { kind: 'WaitForCallback', label: 'B', name: 'a_2', timeoutMs: 1000 });
    const clip = copySelection([w1, w2], [], new Set(['w1', 'w2']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const names = instantiatePaste(clip, [w1, w2]).nodes.map((n) =>
      n.kind.kind === 'WaitForCallback' ? n.kind.name : '',
    );
    expect(new Set(names).size).toBe(2);
    expect(names).not.toContain('a');
    expect(names).not.toContain('a_2');
  });

  it('keeps a saved request path and gives an inline request its own copy', () => {
    const saved = at('s', {
      kind: 'Request',
      label: 'Saved',
      source: { type: 'Saved', requestPath: 'users/get.yml' },
    });
    const inline = at('i', {
      kind: 'Request',
      label: 'Inline',
      source: {
        type: 'Inline',
        request: {
          method: 'GET',
          url: 'https://x.test',
          headers: [{ name: 'a', value: 'b' }],
        },
      },
    });
    const clip = copySelection([saved, inline], [], new Set(['s', 'i']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const [pastedSaved, pastedInline] = instantiatePaste(clip, [saved, inline]).nodes;
    if (pastedSaved.kind.kind !== 'Request' || pastedInline.kind.kind !== 'Request') {
      throw new Error('Expected Request nodes');
    }
    expect(pastedSaved.kind.source).toEqual({ type: 'Saved', requestPath: 'users/get.yml' });
    if (pastedInline.kind.source.type !== 'Inline' || inline.kind.kind !== 'Request') {
      throw new Error('Expected an inline source');
    }
    if (inline.kind.source.type !== 'Inline') throw new Error('Expected an inline source');
    pastedInline.kind.source.request.headers.push({ name: 'x', value: 'y' });
    expect(inline.kind.source.request.headers).toHaveLength(1);
  });
});

describe('uniqueCallbackName', () => {
  it('returns the name when it is free and adds _2, _3 when it is not', () => {
    expect(uniqueCallbackName('pay', new Set())).toBe('pay');
    expect(uniqueCallbackName('pay', new Set(['pay']))).toBe('pay_2');
    expect(uniqueCallbackName('pay', new Set(['pay', 'pay_2']))).toBe('pay_3');
  });
});

describe('canPasteInto', () => {
  const savedNode = at('s', {
    kind: 'Request',
    label: 'S',
    source: { type: 'Saved', requestPath: 'a.yml' },
  });

  it('refuses saved requests from another collection', () => {
    const clip = copySelection([savedNode], [], new Set(['s']), 'one');
    if (!clip) throw new Error('Expected a clip');
    expect(canPasteInto(clip, 'two')).toMatch(/one/);
    expect(canPasteInto(clip, 'one')).toBeNull();
  });

  it('allows other nodes anywhere', () => {
    const clip = copySelection([input('a')], [], new Set(['a']), 'one');
    if (!clip) throw new Error('Expected a clip');
    expect(canPasteInto(clip, 'two')).toBeNull();
  });
});

describe('in-memory clipboard', () => {
  beforeEach(() => clearFlowClipboard());

  it('starts empty and keeps what was set', () => {
    expect(getFlowClipboard()).toBeNull();
    const clip = copySelection([input('a')], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    setFlowClipboard(clip);
    expect(getFlowClipboard()).toBe(clip);
  });

  it('counts pastes and restarts the count on a new copy', () => {
    const clip = copySelection([input('a')], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    setFlowClipboard(clip);
    expect(nextPasteStep()).toBe(1);
    expect(nextPasteStep()).toBe(2);
    setFlowClipboard(clip);
    expect(nextPasteStep()).toBe(1);
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-clipboard.test.ts`
Expected: FAIL, cannot resolve `../flow-clipboard`. If a later run reports `structuredClone is not defined`, add `import { structuredClone } from 'node:util'`-style polyfill only inside `src/test-setup.ts`; Node 20 and jsdom normally provide it.

- [ ] **Step 7: Write the clipboard**

Create `src/lib/flow-clipboard.ts`:

```ts
import { caseHandle, caseIdFromHandle } from '@/lib/flow-handles';
import { newEntityId } from '@/lib/flow-ids';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';

// Pixels a pasted node moves right and down from its original, per paste.
export const PASTE_OFFSET = 40;

export interface FlowClip {
  // The collection the nodes came from. Saved requests only resolve inside it.
  collection: string | null;
  nodes: FlowNode[];
  edges: FlowEdge[];
}

export interface PasteResult {
  nodes: FlowNode[];
  edges: FlowEdge[];
  // Short messages for the user, such as a renamed callback.
  notices: string[];
}

/**
 * Builds a clip from the selected nodes and the wires between them. A wire is
 * kept only when both of its ends are selected. The clip is a deep copy.
 * Returns null when no selected node exists.
 */
export function copySelection(
  nodes: FlowNode[],
  edges: FlowEdge[],
  ids: ReadonlySet<string>,
  collection: string | null,
): FlowClip | null {
  const picked = nodes.filter((n) => ids.has(n.id));
  if (picked.length === 0) return null;
  const kept = new Set(picked.map((n) => n.id));
  return structuredClone({
    collection,
    nodes: picked,
    edges: edges.filter((e) => kept.has(e.sourceNodeId) && kept.has(e.targetNodeId)),
  });
}

/** `name`, then `name_2`, `name_3`, ... whichever is free first. */
export function uniqueCallbackName(name: string, taken: ReadonlySet<string>): string {
  if (!taken.has(name)) return name;
  let i = 2;
  while (taken.has(`${name}_${i}`)) i += 1;
  return `${name}_${i}`;
}

/** A reason the clip cannot go into this collection, or null when it can. */
export function canPasteInto(clip: FlowClip, collection: string | null): string | null {
  const hasSaved = clip.nodes.some(
    (n) => n.kind.kind === 'Request' && n.kind.source.type === 'Saved',
  );
  if (hasSaved && clip.collection !== collection) {
    return `Cannot paste saved requests from "${clip.collection ?? 'another collection'}" into this flow's collection.`;
  }
  return null;
}

/**
 * Makes new nodes and wires from a clip. Every id is new, positions move by
 * `PASTE_OFFSET * step`, and the per-kind rules apply: an Auth node stops
 * applying to inherited auth (V13), a Switch gets new case ids and its wires
 * follow them, and a Wait for callback gets a free name (V10). Inputs are never changed.
 */
export function instantiatePaste(
  clip: FlowClip,
  existingNodes: FlowNode[],
  step = 1,
): PasteResult {
  const fresh = structuredClone({ nodes: clip.nodes, edges: clip.edges });
  const idMap = new Map<string, string>();
  // Old node id, then old case id, to new case id.
  const caseMaps = new Map<string, Map<string, string>>();
  const takenNames = new Set(
    existingNodes.flatMap((n) => (n.kind.kind === 'WaitForCallback' ? [n.kind.name] : [])),
  );
  const notices: string[] = [];
  const offset = PASTE_OFFSET * step;

  const nodes = fresh.nodes.map((node): FlowNode => {
    const id = newEntityId();
    idMap.set(node.id, id);
    const kind = freshKind(node, takenNames, caseMaps, notices);
    return {
      ...node,
      id,
      kind,
      position: { x: node.position.x + offset, y: node.position.y + offset },
    };
  });

  const edges = fresh.edges.flatMap((edge): FlowEdge[] => {
    const sourceNodeId = idMap.get(edge.sourceNodeId);
    const targetNodeId = idMap.get(edge.targetNodeId);
    if (!sourceNodeId || !targetNodeId) return [];
    const pasted: FlowEdge = { ...edge, id: newEntityId(), sourceNodeId, targetNodeId };
    const oldCase = edge.sourceHandle ? caseIdFromHandle(edge.sourceHandle) : null;
    if (oldCase !== null) {
      const newCase = caseMaps.get(edge.sourceNodeId)?.get(oldCase);
      // A wire from a case the node no longer has cannot be kept.
      if (!newCase) return [];
      pasted.sourceHandle = caseHandle(newCase);
    }
    return [pasted];
  });

  return { nodes, edges, notices };
}

// Applies the per-kind paste rules to one cloned node.
function freshKind(
  node: FlowNode,
  takenNames: Set<string>,
  caseMaps: Map<string, Map<string, string>>,
  notices: string[],
): FlowNodeKind {
  const kind = node.kind;
  if (kind.kind === 'Auth') {
    if (kind.applyToInherit) {
      notices.push(
        'The pasted Auth node does not apply to inherited auth. Only one Auth node in a flow can.',
      );
    }
    return { ...kind, applyToInherit: false };
  }
  if (kind.kind === 'Switch') {
    const map = new Map<string, string>();
    const cases = kind.cases.map((c) => {
      const id = newEntityId();
      map.set(c.id, id);
      return { ...c, id };
    });
    caseMaps.set(node.id, map);
    return { ...kind, cases };
  }
  if (kind.kind === 'WaitForCallback') {
    const name = uniqueCallbackName(kind.name, takenNames);
    takenNames.add(name);
    if (name !== kind.name) {
      notices.push(
        `Renamed callback "${kind.name}" to "${name}". A pasted request that sends {{callback.${kind.name}}} still points at the original and may fail validation when you save.`,
      );
    }
    return { ...kind, name };
  }
  return kind;
}

// The clipboard lives in memory only. It survives tab switches but not a reload.
let clipboard: FlowClip | null = null;
let pasteCount = 0;

export function setFlowClipboard(clip: FlowClip): void {
  clipboard = clip;
  pasteCount = 0;
}

export function getFlowClipboard(): FlowClip | null {
  return clipboard;
}

/** The step for the next paste of the current clip: 1, then 2, then 3. */
export function nextPasteStep(): number {
  pasteCount += 1;
  return pasteCount;
}

export function clearFlowClipboard(): void {
  clipboard = null;
  pasteCount = 0;
}
```

- [ ] **Step 8: Run to verify the tests pass**

Run: `yarn test src/lib/__tests__/flow-clipboard.test.ts src/lib/__tests__/flow-ids.test.ts`
Expected: PASS.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/lib/__tests__/flow-clipboard.test.ts src/lib/__tests__/flow-ids.test.ts src/components/flow`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-ids.ts src/lib/__tests__/flow-ids.test.ts src/lib/flow-clipboard.ts src/lib/__tests__/flow-clipboard.test.ts src/components/flow/NodePalette.tsx`
Suggested subject: `feat(flow): add node clipboard with per-kind paste rules`.

---

### Task 2: Keys and handlers

**Files:**
- Modify: `src/components/flow/FlowCanvas.tsx` (props interface, destructured props, `handleKeyDown`, `CANVAS_HINTS`, `nodeActions`)
- Modify: `src/components/flow/nodes/FlowNodeActionsContext.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (imports, one block of handlers, canvas props)
- Create: `src/components/flow/__tests__/FlowCanvas.clipboard.test.tsx`
- Create: `src/components/flow/__tests__/FlowPane.clipboard.test.tsx`

**Interfaces:**
- Consumes: Task 1 exports; P4 `handleSelectedNodeIdsChange`, `latestFlowTab`, `updateFlowGraph`, `tabId` already in `FlowPane`.
- Produces: `FlowCanvas` props `onCopy?: () => void`, `onPaste?: () => void`, `onDuplicate?: () => void`, `onDuplicateNode?: (nodeId: string) => void`; `FlowNodeActions.duplicateNode?: (nodeId: string) => void`.

- [ ] **Step 1: Write the failing canvas key tests**

Create `src/components/flow/__tests__/FlowCanvas.clipboard.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { FlowCanvas } from '../FlowCanvas';

const nodes: FlowNode[] = [
  { id: 'a', kind: { kind: 'Output', label: 'Alpha' }, position: { x: 0, y: 0 } },
  { id: 'b', kind: { kind: 'Output', label: 'Beta' }, position: { x: 300, y: 0 } },
];

function renderCanvas(selected: string[], handlers: Record<string, () => void>) {
  return render(
    <FlowCanvas
      nodes={nodes}
      edges={[]}
      nodeStatus={{}}
      onNodesChange={vi.fn()}
      onEdgesChange={vi.fn()}
      onConnect={vi.fn()}
      selectedNodeIds={new Set(selected)}
      onSelectedNodeIdsChange={vi.fn()}
      {...handlers}
    />,
  );
}

describe('FlowCanvas copy, paste and duplicate keys', () => {
  it('copies and duplicates only with a selection', () => {
    const onCopy = vi.fn();
    const onDuplicate = vi.fn();
    renderCanvas([], { onCopy, onDuplicate });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas, { key: 'd', ctrlKey: true });
    expect(onCopy).not.toHaveBeenCalled();
    expect(onDuplicate).not.toHaveBeenCalled();
  });

  it('calls the handlers on Ctrl and Cmd', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    const onDuplicate = vi.fn();
    renderCanvas(['a'], { onCopy, onPaste, onDuplicate });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas, { key: 'v', metaKey: true });
    fireEvent.keyDown(canvas, { key: 'd', ctrlKey: true });
    expect(onCopy).toHaveBeenCalledTimes(1);
    expect(onPaste).toHaveBeenCalledTimes(1);
    expect(onDuplicate).toHaveBeenCalledTimes(1);
  });

  it('prevents the browser default for paste and duplicate', () => {
    renderCanvas(['a'], { onPaste: vi.fn(), onDuplicate: vi.fn() });
    const canvas = screen.getByTestId('flow-canvas');
    expect(fireEvent.keyDown(canvas, { key: 'v', ctrlKey: true })).toBe(false);
    expect(fireEvent.keyDown(canvas, { key: 'd', ctrlKey: true })).toBe(false);
  });

  it('leaves the keys to a field inside the canvas', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    renderCanvas(['a'], { onCopy, onPaste });
    const input = document.createElement('input');
    screen.getByTestId('flow-canvas').appendChild(input);
    fireEvent.keyDown(input, { key: 'c', ctrlKey: true });
    fireEvent.keyDown(input, { key: 'v', ctrlKey: true });
    expect(onCopy).not.toHaveBeenCalled();
    expect(onPaste).not.toHaveBeenCalled();
  });

  it('ignores Ctrl+Shift+C and Ctrl+Shift+V', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    renderCanvas(['a'], { onCopy, onPaste });
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.keyDown(canvas, { key: 'C', ctrlKey: true, shiftKey: true });
    fireEvent.keyDown(canvas, { key: 'V', ctrlKey: true, shiftKey: true });
    expect(onCopy).not.toHaveBeenCalled();
    expect(onPaste).not.toHaveBeenCalled();
  });

  it('does nothing without handlers', () => {
    renderCanvas(['a'], {});
    expect(() =>
      fireEvent.keyDown(screen.getByTestId('flow-canvas'), { key: 'v', ctrlKey: true }),
    ).not.toThrow();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowCanvas.clipboard.test.tsx`
Expected: FAIL (the handlers are never called).

- [ ] **Step 3: Add the action to the context**

In `src/components/flow/nodes/FlowNodeActionsContext.tsx`, add to `interface FlowNodeActions`, after `openProperties`:

```tsx
  /** Duplicates just this node. Absent when the canvas cannot duplicate. */
  duplicateNode?: (nodeId: string) => void;
```

Leave the default context value (`noop` for the three required functions) unchanged.

- [ ] **Step 4: Change `FlowCanvas.tsx`**

4a. In `FlowCanvasProps`, after the P4 `onRedo?: () => void;` line add:

```tsx
  // Ctrl+C, Ctrl+V and Ctrl+D. Copy and duplicate only fire with a selection.
  onCopy?: () => void;
  onPaste?: () => void;
  onDuplicate?: () => void;
  // Duplicates one node, for its menu entry.
  onDuplicateNode?: (nodeId: string) => void;
```

4b. In the `FlowCanvasInner` destructured props add `onCopy, onPaste, onDuplicate, onDuplicateNode,` after the P4 `onRedo,`.

4c. In `handleKeyDown`, add these branches after the P4 `key === 'y'` branch (inside the same function, before its closing brace). The `y` branch has no `return` after `onRedo();`, so add one there first:

Replace

```tsx
    if (key === 'y' && onRedo) {
      e.preventDefault();
      onRedo();
    }
  };
```

with

```tsx
    if (key === 'y' && onRedo) {
      e.preventDefault();
      onRedo();
      return;
    }
    // Plain Ctrl+C, V and D only. Shift variants belong to the browser.
    if (e.shiftKey) return;
    if (key === 'c' && onCopy && selectedNodeIds.size > 0) {
      e.preventDefault();
      onCopy();
      return;
    }
    if (key === 'v' && onPaste) {
      e.preventDefault();
      onPaste();
      return;
    }
    if (key === 'd' && onDuplicate) {
      // Ctrl+D is a bookmark shortcut in some webviews, so always stop it.
      e.preventDefault();
      if (selectedNodeIds.size > 0) onDuplicate();
    }
  };
```

Note: the `if (e.shiftKey) return;` line must sit after the P4 `z` branch, because Ctrl+Shift+Z is redo.

4d. In the `CANVAS_HINTS` array (P4) append two entries so it ends with:

```tsx
  'Ctrl+Z undo',
  'Ctrl+C/V copy and paste',
  'Ctrl+D duplicate',
];
```

4e. In the `nodeActions` `useMemo`, add `duplicateNode: onDuplicateNode,` after the `openProperties` entry, and change the dependency list `[onNodeKindChange, onRemoveSwitchCase]` to `[onNodeKindChange, onRemoveSwitchCase, onDuplicateNode]`.

- [ ] **Step 5: Run the canvas tests**

Run: `yarn test src/components/flow/__tests__/FlowCanvas.clipboard.test.tsx src/components/flow/__tests__/FlowCanvas.history.test.tsx src/components/flow/__tests__/FlowCanvas.test.tsx`
Expected: PASS.

- [ ] **Step 6: Write the failing FlowPane tests**

Create `src/components/flow/__tests__/FlowPane.clipboard.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_AUTH_NODE_AUTH } from '@/lib/flow-auth';
import { clearFlowClipboard, getFlowClipboard, PASTE_OFFSET } from '@/lib/flow-clipboard';
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

vi.mock('@/components/editor', () => ({ SingleLineEditor: () => null }));

// jsdom cannot drive React Flow's gestures, so the canvas is a stand-in that
// exposes the props FlowPane gives it.
interface CanvasProps {
  selectedNodeIds?: ReadonlySet<string>;
  onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void;
  onCopy?: () => void;
  onPaste?: () => void;
  onDuplicate?: () => void;
  onDuplicateNode?: (nodeId: string) => void;
}
let canvas: CanvasProps = {};
vi.mock('../FlowCanvas', () => ({
  FlowCanvas: (props: CanvasProps) => {
    canvas = props;
    return null;
  },
}));

const TAB_ID = 'flow-clip-1';

const node = (id: string, kind: FlowNode['kind'], x = 0, y = 0): FlowNode => ({
  id,
  kind,
  position: { x, y },
});
const output = (id: string, x = 0, y = 0) => node(id, { kind: 'Output', label: id }, x, y);
const wire = (id: string, from: string, to: string, over: Partial<FlowEdge> = {}): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'value',
  expression: 'response.body',
  ...over,
});

function seed(nodes: FlowNode[], edges: FlowEdge[] = [], over: Partial<FlowTab> = {}) {
  usePaneStore.getState().openTab({
    id: TAB_ID,
    tabType: 'flow',
    title: 'Flow: clip',
    isDirty: false,
    collectionName: 'demo',
    flowName: 'clip',
    nodes,
    edges,
    nodeStatus: {},
    runState: 'idle',
    ...over,
  });
}

function current(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === TAB_ID);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the flow tab');
  return tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === TAB_ID);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const select = (...ids: string[]) =>
  act(() => canvas.onSelectedNodeIdsChange?.(new Set(ids)));

describe('FlowPane copy, paste and duplicate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    clearFlowClipboard();
    canvas = {};
  });

  it('pastes copied nodes and wires as new nodes and selects them', () => {
    seed([output('a', 0, 0), output('b', 100, 0)], [wire('e1', 'a', 'b', { targetField: 'trigger' })]);
    render(<Harness />);
    select('a', 'b');
    act(() => canvas.onCopy?.());
    expect(getFlowClipboard()?.nodes).toHaveLength(2);
    act(() => canvas.onPaste?.());
    const tab = current();
    expect(tab.nodes).toHaveLength(4);
    expect(tab.edges).toHaveLength(2);
    const added = tab.nodes.slice(2);
    expect(added.map((n) => n.position)).toEqual([
      { x: PASTE_OFFSET, y: PASTE_OFFSET },
      { x: 100 + PASTE_OFFSET, y: PASTE_OFFSET },
    ]);
    expect(tab.edges[1].sourceNodeId).toBe(added[0].id);
    expect(tab.edges[1].targetNodeId).toBe(added[1].id);
    expect(canvas.selectedNodeIds).toEqual(new Set(added.map((n) => n.id)));
    expect(tab.isDirty).toBe(true);
  });

  it('makes one paste one undo step', () => {
    seed([output('a'), output('b', 100)], [wire('e1', 'a', 'b', { targetField: 'trigger' })]);
    render(<Harness />);
    select('a', 'b');
    act(() => canvas.onCopy?.());
    act(() => canvas.onPaste?.());
    expect(current().history?.past).toHaveLength(1);
    act(() => usePaneStore.getState().undoFlow(TAB_ID));
    expect(current().nodes.map((n) => n.id)).toEqual(['a', 'b']);
    expect(current().edges.map((e) => e.id)).toEqual(['e1']);
  });

  it('moves each repeated paste further away', () => {
    seed([output('a', 0, 0)]);
    render(<Harness />);
    select('a');
    act(() => canvas.onCopy?.());
    act(() => canvas.onPaste?.());
    act(() => canvas.onPaste?.());
    const [, first, second] = current().nodes;
    expect(first.position).toEqual({ x: PASTE_OFFSET, y: PASTE_OFFSET });
    expect(second.position).toEqual({ x: 2 * PASTE_OFFSET, y: 2 * PASTE_OFFSET });
  });

  it('does nothing when nothing is selected or the clipboard is empty', () => {
    seed([output('a')]);
    render(<Harness />);
    act(() => canvas.onCopy?.());
    expect(getFlowClipboard()).toBeNull();
    act(() => canvas.onPaste?.());
    act(() => canvas.onDuplicate?.());
    expect(current().nodes).toHaveLength(1);
    expect(current().isDirty).toBe(false);
  });

  it('pastes into another flow tab through the shared clipboard', () => {
    seed([output('a', 5, 5)]);
    const first = render(<Harness />);
    select('a');
    act(() => canvas.onCopy?.());
    first.unmount();
    usePaneStore.getState().reset();
    seed([output('z')]);
    render(<Harness />);
    act(() => canvas.onPaste?.());
    expect(current().nodes).toHaveLength(2);
    expect(current().nodes[0].id).toBe('z');
    expect(current().nodes[1].position).toEqual({ x: 5 + PASTE_OFFSET, y: 5 + PASTE_OFFSET });
  });

  it('refuses saved requests from another collection', () => {
    const saved = node('s', {
      kind: 'Request',
      label: 'S',
      source: { type: 'Saved', requestPath: 'a.yml' },
    });
    seed([saved], [], { collectionName: 'one' });
    const first = render(<Harness />);
    select('s');
    act(() => canvas.onCopy?.());
    first.unmount();
    usePaneStore.getState().reset();
    seed([output('z')], [], { collectionName: 'two' });
    render(<Harness />);
    act(() => canvas.onPaste?.());
    expect(current().nodes).toHaveLength(1);
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('one'));
  });

  it('duplicates the selection without touching the clipboard', () => {
    seed([output('a', 0, 0)]);
    render(<Harness />);
    select('a');
    act(() => canvas.onDuplicate?.());
    expect(current().nodes).toHaveLength(2);
    expect(getFlowClipboard()).toBeNull();
    expect(canvas.selectedNodeIds?.size).toBe(1);
    expect(canvas.selectedNodeIds?.has('a')).toBe(false);
  });

  it('duplicates one node from its menu even when it is not selected', () => {
    seed([output('a'), output('b', 100)]);
    render(<Harness />);
    select('b');
    act(() => canvas.onDuplicateNode?.('a'));
    expect(current().nodes).toHaveLength(3);
    expect(current().nodes[2].kind).toEqual({ kind: 'Output', label: 'a' });
  });

  it('pastes an Auth node that does not apply to inherited auth and tells the user', () => {
    const auth = node('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: DEFAULT_AUTH_NODE_AUTH,
      applyToInherit: true,
    });
    seed([auth]);
    render(<Harness />);
    select('auth');
    act(() => canvas.onDuplicate?.());
    const kinds = current().nodes.map((n) => n.kind);
    expect(kinds.filter((k) => k.kind === 'Auth' && k.applyToInherit)).toHaveLength(1);
    expect(toast.info).toHaveBeenCalledWith(expect.stringMatching(/inherited auth/i));
  });

  it('keeps a pasted Switch wired to its own case exits', () => {
    const sw = node('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [{ id: 'c1', label: 'One', matches: '1' }],
    });
    seed(
      [sw, output('o1', 300, 0)],
      [wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('c1'), targetField: 'trigger' })],
    );
    render(<Harness />);
    select('sw', 'o1');
    act(() => canvas.onDuplicate?.());
    const tab = current();
    const pasted = tab.nodes[2].kind;
    if (pasted.kind !== 'Switch') throw new Error('Expected a Switch node');
    expect(pasted.cases[0].id).not.toBe('c1');
    expect(tab.edges[1].sourceHandle).toBe(caseHandle(pasted.cases[0].id));
    expect(tab.edges[0].sourceHandle).toBe(caseHandle('c1'));
  });

  it('renames a pasted Wait for callback and warns about the old variable', () => {
    const wait = node('w', {
      kind: 'WaitForCallback',
      label: 'Wait',
      name: 'pay',
      timeoutMs: 60000,
    });
    seed([wait]);
    render(<Harness />);
    select('w');
    act(() => canvas.onDuplicate?.());
    const names = current().nodes.map((n) => (n.kind.kind === 'WaitForCallback' ? n.kind.name : ''));
    expect(names).toEqual(['pay', 'pay_2']);
    expect(toast.info).toHaveBeenCalledWith(expect.stringContaining('{{callback.pay}}'));
  });

  it('does not open the properties panel for the pasted nodes', () => {
    seed([output('a')]);
    render(<Harness />);
    select('a');
    act(() => canvas.onDuplicate?.());
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 7: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowPane.clipboard.test.tsx`
Expected: FAIL (`canvas.onCopy` is undefined).

- [ ] **Step 8: Wire `FlowPane.tsx`**

8a. Imports. Add after the P4 `pruneSelection` import (sorted order by Biome):

```tsx
import {
  canPasteInto,
  copySelection,
  type FlowClip,
  getFlowClipboard,
  instantiatePaste,
  nextPasteStep,
  setFlowClipboard,
} from '@/lib/flow-clipboard';
```

8b. Handlers. Insert directly after the P4 `handleGestureEnd` `useCallback` (and before the P4 prune `useEffect`). They are hooks, so they must stay above the early return for the picker state:

```tsx
  // Adds a clip to the flow as new nodes in one store write, which is one undo step.
  const pasteClip = useCallback(
    (clip: FlowClip, step: number) => {
      const latest = latestFlowTab();
      if (!latest) return;
      const blocked = canPasteInto(clip, latest.collectionName);
      if (blocked) {
        toast.error(blocked);
        return;
      }
      const result = instantiatePaste(clip, latest.nodes, step);
      updateFlowGraph(
        tabId,
        [...latest.nodes, ...result.nodes],
        [...latest.edges, ...result.edges],
      );
      // The pasted nodes become the selection. The panel stays closed.
      handleSelectedNodeIdsChange(new Set(result.nodes.map((n) => n.id)));
      for (const notice of result.notices) toast.info(notice);
    },
    [latestFlowTab, tabId, updateFlowGraph, handleSelectedNodeIdsChange],
  );

  const handleCopy = useCallback(() => {
    const latest = latestFlowTab();
    if (!latest) return;
    const clip = copySelection(latest.nodes, latest.edges, selectedNodeIds, latest.collectionName);
    if (!clip) return;
    setFlowClipboard(clip);
    toast.info(clip.nodes.length === 1 ? 'Copied 1 node.' : `Copied ${clip.nodes.length} nodes.`);
  }, [latestFlowTab, selectedNodeIds]);

  const handlePaste = useCallback(() => {
    const clip = getFlowClipboard();
    if (clip) pasteClip(clip, nextPasteStep());
  }, [pasteClip]);

  // Duplicating copies and pastes in one go and leaves the clipboard alone.
  const duplicateNodes = useCallback(
    (ids: ReadonlySet<string>) => {
      const latest = latestFlowTab();
      if (!latest) return;
      const clip = copySelection(latest.nodes, latest.edges, ids, latest.collectionName);
      if (clip) pasteClip(clip, 1);
    },
    [latestFlowTab, pasteClip],
  );
  const handleDuplicate = useCallback(
    () => duplicateNodes(selectedNodeIds),
    [duplicateNodes, selectedNodeIds],
  );
  const handleDuplicateNode = useCallback(
    (nodeId: string) => duplicateNodes(new Set([nodeId])),
    [duplicateNodes],
  );
```

8c. Canvas props. In the `<FlowCanvas` element add after the P4 `onRedo={handleRedo}` line:

```tsx
            onCopy={handleCopy}
            onPaste={handlePaste}
            onDuplicate={handleDuplicate}
            onDuplicateNode={handleDuplicateNode}
```

- [ ] **Step 9: Run to verify the tests pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-clipboard.test.ts`
Expected: PASS. In the multiselect and other existing `FlowPane` tests the stand-in canvas ignores the new props, so they stay green.

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib/__tests__`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/FlowCanvas.tsx src/components/flow/nodes/FlowNodeActionsContext.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowCanvas.clipboard.test.tsx src/components/flow/__tests__/FlowPane.clipboard.test.tsx`
Suggested subject: `feat(flow): copy, paste and duplicate nodes with Ctrl+C, V and D`.

---

### Task 3: Node menu entry and end-to-end tests on the real canvas

**Files:**
- Modify: `src/components/flow/nodes/NodeMenuButton.tsx`
- Create: `src/components/flow/__tests__/FlowPane.clipboardKeys.test.tsx`

**Interfaces:**
- Consumes: `FlowNodeActions.duplicateNode` (Task 2).

- [ ] **Step 1: Write the failing end-to-end tests**

Create `src/components/flow/__tests__/FlowPane.clipboardKeys.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearFlowClipboard } from '@/lib/flow-clipboard';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
  };
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

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
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

// Park the resize handle away from the pointer, as the delete test does.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const baseTab: FlowTab = {
  id: 'flow-clipkeys-1',
  tabType: 'flow',
  title: 'Flow: keys',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'keys',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    {
      id: 'req1',
      position: { x: 0, y: 200 },
      kind: {
        kind: 'Request',
        label: 'Login',
        source: { type: 'Saved', requestPath: 'login.yml' },
      },
    },
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

function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab');
  return tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const canvas = () => screen.getByTestId('flow-canvas');
const count = () => getFlowTab().nodes.length;

describe('FlowPane clipboard keys on the real canvas', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    clearFlowClipboard();
  });

  it('copies with Ctrl+C and pastes with Ctrl+V, selecting the new node', async () => {
    render(<Harness />);
    // User-event mouse events have a null view, which d3-drag rejects on a node body.
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'v', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(4));
    const pasted = getFlowTab().nodes[3];
    expect(pasted.position).toEqual({ x: 340, y: 40 });
    await waitFor(() =>
      expect(document.querySelector(`.react-flow__node[data-id="${pasted.id}"]`)).toHaveClass(
        'selected',
      ),
    );
    expect(document.querySelector('.react-flow__node[data-id="out1"]')).not.toHaveClass('selected');
  });

  it('undoes a paste in one Ctrl+Z', async () => {
    render(<Harness />);
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'v', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'v', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(5));
    fireEvent.keyDown(canvas(), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(4));
    fireEvent.keyDown(canvas(), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(3));
  });

  it('duplicates the wired pair with Ctrl+D', async () => {
    render(<Harness />);
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'a', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'd', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(6));
    const tab = getFlowTab();
    expect(tab.edges).toHaveLength(2);
    const ids = new Set(tab.nodes.map((n) => n.id));
    expect(ids.size).toBe(6);
    for (const e of tab.edges) {
      expect(ids.has(e.sourceNodeId)).toBe(true);
      expect(ids.has(e.targetNodeId)).toBe(true);
    }
    expect(tab.edges[1].sourceNodeId).not.toBe('in1');
    expect(tab.edges[1].targetNodeId).not.toBe('out1');
  });

  it('leaves Ctrl+V to a field inside the canvas', () => {
    render(<Harness />);
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'c', ctrlKey: true });
    const input = document.createElement('input');
    canvas().appendChild(input);
    fireEvent.keyDown(input, { key: 'v', ctrlKey: true });
    expect(count()).toBe(3);
  });

  it('duplicates a request from its node menu', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Login'));
    await user.click(await screen.findByRole('menuitem', { name: 'Duplicate' }));
    await waitFor(() => expect(count()).toBe(4));
    const copy = getFlowTab().nodes[3].kind;
    if (copy.kind !== 'Request') throw new Error('Expected a Request node');
    expect(copy.source).toEqual({ type: 'Saved', requestPath: 'login.yml' });
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowPane.clipboardKeys.test.tsx`
Expected: the four key tests PASS (Task 2 already wired them) and the menu test FAILS (no "Duplicate" item).

- [ ] **Step 3: Add the menu item**

In `src/components/flow/nodes/NodeMenuButton.tsx`:

3a. Change `const { openProperties } = useFlowNodeActions();` to:

```tsx
  const { openProperties, duplicateNode } = useFlowNodeActions();
```

3b. In the `DropdownMenuContent`, add this item after the `Edit properties` item and before the `DropdownMenuCheckboxItem`:

```tsx
        {duplicateNode && (
          <DropdownMenuItem onSelect={() => duplicateNode(nodeId)}>Duplicate</DropdownMenuItem>
        )}
```

Only nodes that pass a `debug` prop get this menu today (Request nodes). Other kinds open their properties directly, so Ctrl+D is their way to duplicate. Do not change that behaviour here: turning every node button into a menu would break the open-on-click flows that the existing tests rely on.

- [ ] **Step 4: Run to verify the tests pass**

Run: `yarn test src/components/flow`
Expected: PASS. If the Ctrl+D test cannot select everything because the first click did not focus the canvas, mirror `FlowPane.delete.test.tsx` (same click target, then `fireEvent.keyDown` on `flow-canvas`).

- [ ] **Step 5: Manual check for the human**

Run `yarn tauri dev`, open a flow with an Auth node, a Switch with wired cases and a Wait for callback node. Select all, Ctrl+C, Ctrl+V. Expected: the copy sits down and right of the original, is selected, keeps its internal wires, the Auth copy shows "applies to inherited auth" off, the Switch copy's wires leave from its own case exits, and the Wait copy is named `<name>_2`. Press Ctrl+Z once: the whole copy disappears. Save: no V10 or V13 error.

- [ ] **Step 6: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib/__tests__`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/nodes/NodeMenuButton.tsx src/components/flow/__tests__/FlowPane.clipboardKeys.test.tsx`
Suggested subject: `feat(flow): add Duplicate to the request node menu and key tests`.

---

## Self-Review

- **Spec coverage:** F-35 in-memory clipboard (Task 1); Ctrl+C, V, D with the editable guard (Task 2); wires only between copied nodes (Task 1); fresh node and edge ids, offset growing per paste (Task 1, 2); one `updateFlowGraph` call and one undo step (Task 2, 3); select the pasted nodes, panel not opened (Task 2); Auth `applyToInherit: false` and no `flow-auth-store` access (Task 1, 2); Switch case id remap (Task 1, 2); Wait name uniqueness with `_2`, `_3` and the V15 toast (Task 1, 2); Request deep clone, Saved path kept (Task 1); id helper extraction (Task 1); hint panel updated through `CANVAS_HINTS` (Task 2); Duplicate in the node menu (Task 3, limited to the node kind that has a menu).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowClip`, `PasteResult`, `copySelection`, `instantiatePaste`, `canPasteInto`, `getFlowClipboard`, `setFlowClipboard`, `nextPasteStep` have the same signatures in `flow-clipboard.ts`, the unit tests and `FlowPane`. `FlowCanvas` prop names (`onCopy`, `onPaste`, `onDuplicate`, `onDuplicateNode`) match the stand-in canvas in the FlowPane test and the `FlowPane` JSX. `duplicateNode` is optional on `FlowNodeActions`, so tests that build the action object by hand still type-check.
- **Review Focus coverage:** item 1 Task 1 (`pastes an Auth node with applyToInherit off`) and Task 2 (inherited auth test); item 2 Task 1 and Task 2 (Switch); item 3 Task 1 (`renames`, `never gives two pasted Wait nodes the same name`) and Task 2; item 4 Task 2 (single undo, growing offset) and Task 3 (keys on the real canvas); item 5 Task 1 (`keeps only wires whose two ends are both selected`, deep copy, inline request clone).
- **Differences from the design notes:** added `canPasteInto` because a saved request path only resolves inside its own collection (the canvas drop handler already rejects cross-collection drops for the same reason). The Duplicate menu entry exists only on Request nodes, because `NodeMenuButton` renders a menu only when given `debug`.

Known follow-ups outside this plan: cut (Ctrl+X); pasted `{{callback.<old>}}` text inside requests is not rewritten; pasting a saved request into a flow of the same collection works but is not checked for a request that was deleted since the copy.
